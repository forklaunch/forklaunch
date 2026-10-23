use super::{ExportBatch, SubjectExportSink, SubjectExportStore, SubjectRequest};
use crate::encryption::EnvelopeKey;
use crate::{
    common::{FrameworkError, FrameworkResult},
};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{
    future::Future,
    io::Write,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

const MAX_DOCUMENT: usize = 1_048_552;
const MAX_SNAPSHOT: usize = 16_777_216;
/// Fixed authorization purpose. Implementations bind verified actor/recipient,
/// validate policy, and acknowledge durable intent before returning success.
#[derive(Clone, Copy, Debug)]
pub enum ExportAction {
    CaptureSnapshot,
    ReadSnapshot,
    Deliver,
    ReadDelivery,
    PurgeExpiredDelivery,
    PurgeExpiredSnapshot,
}
pub trait ExportAuthorization: Send + Sync {
    fn authorize<'a>(
        &'a self,
        request: &'a SubjectRequest<'_>,
        recipient: Option<&'a str>,
        action: ExportAction,
    ) -> Pin<Box<dyn Future<Output = FrameworkResult<()>> + Send + 'a>>;
}
/// Canonical JSON held in a wiped buffer; only an authorized snapshot/destination
/// can construct it. Debug never exposes the projection. Decode deliberately
/// creates application-owned typed values after authorization and audit.
pub struct ExportDocument(Zeroizing<Vec<u8>>);
impl std::fmt::Debug for ExportDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExportDocument([redacted])")
    }
}
impl ExportDocument {
    #[must_use]
    pub fn json_bytes(&self) -> &[u8] {
        &self.0
    }
    /// # Errors
    /// Rejects mismatched destination DTOs with a fixed non-sensitive error.
    pub fn decode<T: DeserializeOwned>(&self) -> FrameworkResult<T> {
        serde_json::from_slice(&self.0).map_err(|_| error("invalid_export_document"))
    }
}
/// Counts only physical source projection rows removed by this invocation.
/// The non-plaintext operation tombstone remains to reject future recreation.
#[derive(Debug, PartialEq, Eq)]
pub struct SnapshotCleanup {
    pub deleted: usize,
    pub complete: bool,
}
pub struct PostgresExportSnapshot {
    pool: PgPool,
    key: Arc<EnvelopeKey>,
    authorization: Arc<dyn ExportAuthorization>,
}
impl PostgresExportSnapshot {
    #[must_use]
    pub fn new(
        pool: PgPool,
        key: Arc<EnvelopeKey>,
        authorization: Arc<dyn ExportAuthorization>,
    ) -> Self {
        Self {
            pool,
            key,
            authorization,
        }
    }
    /// Owner-run migration. Grant restricted runtime SELECT/INSERT on records,
    /// SELECT/INSERT/UPDATE on snapshots; no record UPDATE or DELETE. Tenant and
    /// operation are UUIDs. Retain configured keys for unfinished exports.
    #[must_use]
    pub const fn migration_sql() -> &'static str {
        "CREATE TABLE forklaunch_export_snapshots(tenant_id uuid NOT NULL,operation_id uuid NOT NULL,scope_tag text NOT NULL,snapshot_tag text NOT NULL,record_count integer NOT NULL CHECK(record_count BETWEEN 0 AND 10000),batch_size integer NOT NULL CHECK(batch_size BETWEEN 1 AND 10000),next_position integer NOT NULL DEFAULT 0 CHECK(next_position>=0),expires_epoch bigint NOT NULL,expiry_tag text NOT NULL,retired boolean NOT NULL DEFAULT false,PRIMARY KEY(tenant_id,operation_id)); CREATE TABLE forklaunch_export_records(tenant_id uuid NOT NULL,operation_id uuid NOT NULL,position integer NOT NULL,record_id uuid NOT NULL,ciphertext text NOT NULL,PRIMARY KEY(tenant_id,operation_id,position),FOREIGN KEY(tenant_id,operation_id) REFERENCES forklaunch_export_snapshots(tenant_id,operation_id)); ALTER TABLE forklaunch_export_snapshots ENABLE ROW LEVEL SECURITY; ALTER TABLE forklaunch_export_snapshots FORCE ROW LEVEL SECURITY; CREATE POLICY export_snapshots_tenant ON forklaunch_export_snapshots USING(tenant_id::text=current_setting('forklaunch.tenant_id',true)) WITH CHECK(tenant_id::text=current_setting('forklaunch.tenant_id',true)); ALTER TABLE forklaunch_export_records ENABLE ROW LEVEL SECURITY; ALTER TABLE forklaunch_export_records FORCE ROW LEVEL SECURITY; CREATE POLICY export_records_tenant ON forklaunch_export_records USING(tenant_id::text=current_setting('forklaunch.tenant_id',true)) WITH CHECK(tenant_id::text=current_setting('forklaunch.tenant_id',true));"
    }
    /// Atomically persist a bounded materialized snapshot of already-authorized
    /// projections. Capture source rows consistently and audit their plaintext
    /// reads before calling this method. No live-table cursor can replace that
    /// source responsibility. Repeated preparation requires identical content.
    /// # Errors
    /// Rejects oversized snapshots, failed authorization, reused scope or changed
    /// content or expiry. The caller chooses an absolute retention deadline, which
    /// cannot be extended by retry. Failed inserts roll back the complete snapshot.
    pub async fn prepare<T: Serialize>(
        &self,
        request: &SubjectRequest<'_>,
        records: &[T],
        expires: SystemTime,
    ) -> FrameworkResult<()> {
        let (tenant, operation) = scope(request)?;
        let expires = epoch(expires)?;
        if expires <= now()? {
            return Err(error("export_snapshot_expired"));
        }
        let expiry_tag = self
            .key
            .sign_public_metadata(
                tenant,
                operation,
                "subject-export-expiry",
                &expiry_binding(request, expires)?,
            )
            .map_err(|_| error("export_key_unavailable"))?;

        if records.len() > 10_000 {
            return Err(error("export_snapshot_too_large"));
        }
        self.authorization
            .authorize(request, None, ExportAction::CaptureSnapshot)
            .await?;
        let mut total = 0usize;
        let mut hashes = Vec::new();
        let mut rows = Vec::new();
        for (position, record) in records.iter().enumerate() {
            let bytes = canonical(record)?;
            total = total.saturating_add(bytes.len());
            if total > MAX_SNAPSHOT {
                return Err(error("export_snapshot_too_large"));
            }
            hashes.extend_from_slice(&Sha256::digest(&bytes));
            let mut payload = Zeroizing::new(Vec::with_capacity(bytes.len() + 24));
            payload.extend_from_slice(operation.as_bytes());
            payload.extend_from_slice(
                &u64::try_from(position)
                    .map_err(|_| error("invalid_export_snapshot"))?
                    .to_be_bytes(),
            );
            payload.extend_from_slice(&bytes);
            let id = Uuid::new_v4();
            let sealed = self
                .key
                .seal(tenant, id, "subject-export-row", &payload)
                .map_err(|_| error("export_key_unavailable"))?;
            rows.push((id, sealed));
        }
        let binding = snapshot_binding(request, records.len())?;
        let scope_tag = self
            .key
            .sign_public_metadata(tenant, operation, "subject-export-scope", &binding)
            .map_err(|_| error("export_key_unavailable"))?;
        let snapshot_tag = self
            .key
            .sign_public_metadata(tenant, operation, "subject-export-snapshot", &hashes)
            .map_err(|_| error("export_key_unavailable"))?;
        let mut tx = transaction(&self.pool, request).await?;
        sqlx::query("INSERT INTO forklaunch_export_snapshots(tenant_id,operation_id,scope_tag,snapshot_tag,record_count,batch_size,expires_epoch,expiry_tag) VALUES($1::uuid,$2::uuid,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(&scope_tag).bind(&snapshot_tag).bind(i32::try_from(records.len()).map_err(|_|error("invalid_export_snapshot"))?).bind(i32::try_from(request.batch_size).map_err(|_|error("invalid_export_snapshot"))?).bind(expires).bind(expiry_tag).execute(&mut *tx).await.map_err(storage)?;
        let row=sqlx::query("SELECT scope_tag,snapshot_tag,record_count,batch_size,expires_epoch,expiry_tag,retired FROM forklaunch_export_snapshots WHERE tenant_id=$1::uuid AND operation_id=$2::uuid FOR UPDATE")
            .bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(storage)?;
        validate_snapshot(&self.key, tenant, operation, request, &row)?;
        active_snapshot(&self.key, tenant, operation, request, &row)?;
        if row.try_get::<i64, _>("expires_epoch").map_err(storage)? != expires {
            return Err(error("export_expiry_changed"));
        }
        let stored: String = row.try_get("snapshot_tag").map_err(storage)?;
        self.key
            .verify_public_metadata(
                tenant,
                operation,
                "subject-export-snapshot",
                &hashes,
                &stored,
            )
            .map_err(|_| error("export_snapshot_changed"))?;
        for (position, (id, ciphertext)) in rows.into_iter().enumerate() {
            sqlx::query("INSERT INTO forklaunch_export_records(tenant_id,operation_id,position,record_id,ciphertext) VALUES($1::uuid,$2::uuid,$3,$4::uuid,$5) ON CONFLICT DO NOTHING")
                .bind(tenant.to_string()).bind(operation.to_string()).bind(i32::try_from(position).map_err(|_|error("invalid_export_snapshot"))?).bind(id.to_string()).bind(ciphertext).execute(&mut *tx).await.map_err(storage)?;
        }
        tx.commit().await.map_err(storage)
    }
    /// Physically remove at most `batch_size` source records once the immutable
    /// deadline expires. Require separate cleanup DELETE rights on records and
    /// SELECT/UPDATE on snapshots. Authorization must durably audit cleanup intent.
    /// Preserve the non-plaintext snapshot tombstone for the entire operation-ID
    /// retry lifetime (indefinitely if IDs can ever be replayed); no API deletes it.
    /// Retired snapshots can never be recaptured, including after partial cleanup.
    /// This does not delete delivered mailboxes or database backups.
    /// # Errors
    /// Denies early cleanup, failed authorization, altered expiry/scope or storage.
    pub async fn purge_expired(
        &self,
        request: &SubjectRequest<'_>,
    ) -> FrameworkResult<SnapshotCleanup> {
        let (tenant, operation) = scope(request)?;
        self.authorization
            .authorize(request, None, ExportAction::PurgeExpiredSnapshot)
            .await?;
        let mut tx = transaction(&self.pool, request).await?;
        let row = sqlx::query("SELECT scope_tag,record_count,batch_size,expires_epoch,expiry_tag,retired FROM forklaunch_export_snapshots WHERE tenant_id=$1::uuid AND operation_id=$2::uuid FOR UPDATE")
            .bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(storage)?;
        validate_snapshot(&self.key, tenant, operation, request, &row)?;
        let expires = authenticated_expiry(&self.key, tenant, operation, request, &row)?;
        if expires > now()? {
            return Err(error("export_snapshot_retained"));
        }
        sqlx::query("UPDATE forklaunch_export_snapshots SET retired=true WHERE tenant_id=$1::uuid AND operation_id=$2::uuid")
            .bind(tenant.to_string()).bind(operation.to_string()).execute(&mut *tx).await.map_err(storage)?;
        let deleted = sqlx::query("DELETE FROM forklaunch_export_records WHERE tenant_id=$1::uuid AND operation_id=$2::uuid AND position IN (SELECT position FROM forklaunch_export_records WHERE tenant_id=$1::uuid AND operation_id=$2::uuid ORDER BY position LIMIT $3)")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(i64::try_from(request.batch_size).map_err(|_|error("invalid_export_scope"))?).execute(&mut *tx).await.map_err(storage)?.rows_affected();
        let remaining: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM forklaunch_export_records WHERE tenant_id=$1::uuid AND operation_id=$2::uuid)")
            .bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(SnapshotCleanup {
            deleted: usize::try_from(deleted).map_err(|_| error("invalid_export_snapshot"))?,
            complete: !remaining,
        })
    }
}
impl SubjectExportStore for PostgresExportSnapshot {
    type Record = ExportDocument;
    async fn next_batch(
        &self,
        request: &SubjectRequest<'_>,
    ) -> FrameworkResult<Option<ExportBatch<ExportDocument>>> {
        let (tenant, operation) = scope(request)?;
        self.authorization
            .authorize(request, None, ExportAction::ReadSnapshot)
            .await?;
        let mut tx = transaction(&self.pool, request).await?;
        let row=sqlx::query("SELECT scope_tag,record_count,batch_size,next_position,expires_epoch,expiry_tag,retired FROM forklaunch_export_snapshots WHERE tenant_id=$1::uuid AND operation_id=$2::uuid")
            .bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(storage)?;
        validate_snapshot(&self.key, tenant, operation, request, &row)?;
        active_snapshot(&self.key, tenant, operation, request, &row)?;
        let start: i32 = row.try_get("next_position").map_err(storage)?;
        let count: i32 = row.try_get("record_count").map_err(storage)?;
        if start >= count {
            tx.commit().await.map_err(storage)?;
            return Ok(None);
        }
        let end = (start
            + i32::try_from(request.batch_size).map_err(|_| error("invalid_export_snapshot"))?)
        .min(count);
        let rows=sqlx::query("SELECT position,record_id::text AS record_id,ciphertext FROM forklaunch_export_records WHERE tenant_id=$1::uuid AND operation_id=$2::uuid AND position >= $3 AND position < $4 ORDER BY position")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(start).bind(end).fetch_all(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        if rows.len()
            != usize::try_from(end - start).map_err(|_| error("invalid_export_snapshot"))?
        {
            return Err(error("invalid_export_snapshot"));
        }
        let mut records = Vec::new();
        for (offset, row) in rows.iter().enumerate() {
            let position: i32 = row.try_get("position").map_err(storage)?;
            if position
                != start + i32::try_from(offset).map_err(|_| error("invalid_export_snapshot"))?
            {
                return Err(error("invalid_export_snapshot"));
            }
            let id: String = row.try_get("record_id").map_err(storage)?;
            let id = Uuid::parse_str(&id).map_err(|_| error("invalid_export_snapshot"))?;
            let ciphertext: String = row.try_get("ciphertext").map_err(storage)?;
            let mut bytes = self
                .key
                .open(tenant, id, "subject-export-row", &ciphertext)
                .map_err(|_| error("invalid_export_snapshot"))?;
            if bytes.len() < 24
                || &bytes[..16] != operation.as_bytes()
                || bytes[16..24]
                    != u64::try_from(position)
                        .map_err(|_| error("invalid_export_snapshot"))?
                        .to_be_bytes()
            {
                return Err(error("invalid_export_snapshot"));
            }
            records.push(ExportDocument(Zeroizing::new(bytes.split_off(24))));
        }
        Ok(Some(ExportBatch {
            batch_id: format!("{start}-{end}"),
            records,
        }))
    }
    async fn acknowledge(
        &self,
        request: &SubjectRequest<'_>,
        batch_id: &str,
    ) -> FrameworkResult<bool> {
        let (tenant, operation) = scope(request)?;
        let (start, end) = page(batch_id)?;
        let mut tx = transaction(&self.pool, request).await?;
        let row=sqlx::query("SELECT scope_tag,record_count,batch_size,next_position,expires_epoch,expiry_tag,retired FROM forklaunch_export_snapshots WHERE tenant_id=$1::uuid AND operation_id=$2::uuid FOR UPDATE")
            .bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(storage)?;
        validate_snapshot(&self.key, tenant, operation, request, &row)?;
        active_snapshot(&self.key, tenant, operation, request, &row)?;
        let count: i32 = row.try_get("record_count").map_err(storage)?;
        let current: i32 = row.try_get("next_position").map_err(storage)?;
        let size = i32::try_from(request.batch_size).map_err(|_| error("invalid_export_page"))?;
        if start % size != 0 || end != (start + size).min(count) || end > count {
            return Err(error("invalid_export_page"));
        }
        if current >= end {
            tx.commit().await.map_err(storage)?;
            return Ok(false);
        }
        if current != start {
            return Err(error("invalid_export_page"));
        }
        sqlx::query("UPDATE forklaunch_export_snapshots SET next_position=$3 WHERE tenant_id=$1::uuid AND operation_id=$2::uuid")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(end).execute(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(true)
    }
}
fn validate_snapshot(
    key: &EnvelopeKey,
    tenant: Uuid,
    operation: Uuid,
    request: &SubjectRequest<'_>,
    row: &sqlx::postgres::PgRow,
) -> FrameworkResult<()> {
    let tag: String = row.try_get("scope_tag").map_err(storage)?;
    key.verify_public_metadata(
        tenant,
        operation,
        "subject-export-scope",
        &snapshot_binding(
            request,
            usize::try_from(row.try_get::<i32, _>("record_count").map_err(storage)?)
                .map_err(|_| error("invalid_export_snapshot"))?,
        )?,
        &tag,
    )
    .map_err(|_| error("export_operation_reused"))?;
    if row.try_get::<i32, _>("batch_size").map_err(storage)?
        != i32::try_from(request.batch_size).map_err(|_| error("invalid_export_page"))?
    {
        return Err(error("export_page_size_changed"));
    }
    Ok(())
}
fn epoch(time: SystemTime) -> FrameworkResult<i64> {
    i64::try_from(
        time.duration_since(UNIX_EPOCH)
            .map_err(|_| error("invalid_export_expiry"))?
            .as_secs(),
    )
    .map_err(|_| error("invalid_export_expiry"))
}
fn expiry_binding(
    request: &SubjectRequest<'_>,
    expires: i64,
) -> FrameworkResult<Zeroizing<Vec<u8>>> {
    let scope = binding(request, None)?;
    serde_json::to_vec(&(scope.as_slice(), expires))
        .map(Zeroizing::new)
        .map_err(|_| error("invalid_export_expiry"))
}
fn authenticated_expiry(
    key: &EnvelopeKey,
    tenant: Uuid,
    operation: Uuid,
    request: &SubjectRequest<'_>,
    row: &sqlx::postgres::PgRow,
) -> FrameworkResult<i64> {
    let expires = row.try_get("expires_epoch").map_err(storage)?;
    let tag: String = row.try_get("expiry_tag").map_err(storage)?;
    key.verify_public_metadata(
        tenant,
        operation,
        "subject-export-expiry",
        &expiry_binding(request, expires)?,
        &tag,
    )
    .map_err(|_| error("invalid_export_expiry"))?;
    Ok(expires)
}
fn active_snapshot(
    key: &EnvelopeKey,
    tenant: Uuid,
    operation: Uuid,
    request: &SubjectRequest<'_>,
    row: &sqlx::postgres::PgRow,
) -> FrameworkResult<()> {
    if authenticated_expiry(key, tenant, operation, request, row)? <= now()?
        || row.try_get::<bool, _>("retired").map_err(storage)?
    {
        return Err(error("export_snapshot_expired"));
    }
    Ok(())
}
fn snapshot_binding(
    request: &SubjectRequest<'_>,
    count: usize,
) -> FrameworkResult<Zeroizing<Vec<u8>>> {
    let scope = binding(request, None)?;
    serde_json::to_vec(&(scope.as_slice(), count))
        .map(Zeroizing::new)
        .map_err(|_| error("invalid_export_snapshot"))
}
fn scope(request: &SubjectRequest<'_>) -> FrameworkResult<(Uuid, Uuid)> {
    request.validate(1)?;
    let tenant = Uuid::parse_str(request.tenant.id()).map_err(|_| error("invalid_export_scope"))?;
    let operation =
        Uuid::parse_str(request.operation_id).map_err(|_| error("invalid_export_scope"))?;
    if tenant.is_nil() || operation.is_nil() {
        return Err(error("invalid_export_scope"));
    }
    Ok((tenant, operation))
}
fn binding(
    request: &SubjectRequest<'_>,
    recipient: Option<&str>,
) -> FrameworkResult<Zeroizing<Vec<u8>>> {
    serde_json::to_vec(&(
        request.entity.entity(),
        request.entity.subject_field(),
        request.subject,
        request.batch_size,
        recipient,
    ))
    .map(Zeroizing::new)
    .map_err(|_| error("invalid_export_scope"))
}
async fn transaction<'a>(
    pool: &'a PgPool,
    request: &SubjectRequest<'_>,
) -> FrameworkResult<sqlx::Transaction<'a, sqlx::Postgres>> {
    let mut tx = begin_tenant_transaction(pool, request.tenant).await?;
    sqlx::query("SELECT set_config('statement_timeout','5000',true),set_config('lock_timeout','2000',true),set_config('synchronous_commit','on',true)").execute(&mut *tx).await.map_err(storage)?;
    Ok(tx)
}
fn page(value: &str) -> FrameworkResult<(i32, i32)> {
    let (a, b) = value
        .split_once('-')
        .ok_or_else(|| error("invalid_export_page"))?;
    let a: i32 = a.parse().map_err(|_| error("invalid_export_page"))?;
    let b: i32 = b.parse().map_err(|_| error("invalid_export_page"))?;
    if a < 0 || b <= a || b > 10000 || value != format!("{a}-{b}") {
        return Err(error("invalid_export_page"));
    }
    Ok((a, b))
}
fn storage(_: sqlx::Error) -> FrameworkError {
    error("export_storage_failed")
}
fn error(code: &str) -> FrameworkError {
    FrameworkError::new(code, "Export operation did not complete")
}

struct Bounded<'a>(&'a mut Vec<u8>, usize);
impl Write for Bounded<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.1.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("export document too large"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct WipedJson(serde_json::Value);
impl Drop for WipedJson {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}
fn wipe(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => s.zeroize(),
        serde_json::Value::Array(v) => {
            for item in v {
                wipe(item);
            }
        }
        serde_json::Value::Object(v) => {
            for (mut key, mut value) in std::mem::take(v) {
                key.zeroize();
                wipe(&mut value);
            }
        }
        _ => {}
    }
    *value = serde_json::Value::Null;
}
fn canonical<T: Serialize>(record: &T) -> FrameworkResult<Zeroizing<Vec<u8>>> {
    let mut raw = Zeroizing::new(Vec::new());
    serde_json::to_writer(Bounded(&mut raw, MAX_DOCUMENT), record)
        .map_err(|_| error("export_document_too_large"))?;
    let value =
        WipedJson(serde_json::from_slice(&raw).map_err(|_| error("invalid_export_document"))?);
    let mut output = Zeroizing::new(Vec::new());
    canonical_value(&value.0, &mut Bounded(&mut output, MAX_DOCUMENT))
        .map_err(|_| error("invalid_export_document"))?;
    Ok(output)
}
fn canonical_value(value: &serde_json::Value, out: &mut Bounded<'_>) -> std::io::Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            out.write_all(b"{")?;
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            for (index, (key, value)) in entries.iter().enumerate() {
                if index > 0 {
                    out.write_all(b",")?;
                }
                serde_json::to_writer(&mut *out, key)?;
                out.write_all(b":")?;
                canonical_value(value, out)?;
            }
            out.write_all(b"}")
        }
        serde_json::Value::Array(array) => {
            out.write_all(b"[")?;
            for (index, value) in array.iter().enumerate() {
                if index > 0 {
                    out.write_all(b",")?;
                }
                canonical_value(value, out)?;
            }
            out.write_all(b"]")
        }
        _ => serde_json::to_writer(out, value).map_err(std::io::Error::other),
    }
}

/// Encrypted durable recipient mailbox. Expiry restricts reads but does not
/// physically purge stored bytes until `purge_expired` is explicitly invoked.
/// Retrying a delivery uses the same recipient and absolute expiry, never a new
/// sliding TTL. Authorization must be re-evaluated on every delivery/read.
pub struct PostgresExportMailbox {
    pool: PgPool,
    key: Arc<EnvelopeKey>,
    authorization: Arc<dyn ExportAuthorization>,
    recipient: Zeroizing<String>,
    expires: i64,
}
impl PostgresExportMailbox {
    /// # Errors
    /// Rejects missing/invalid recipients and expired/nonrepresentable deadlines.
    pub fn new(
        pool: PgPool,
        key: Arc<EnvelopeKey>,
        authorization: Arc<dyn ExportAuthorization>,
        recipient: String,
        expires: SystemTime,
    ) -> FrameworkResult<Self> {
        let expires = i64::try_from(
            expires
                .duration_since(UNIX_EPOCH)
                .map_err(|_| error("invalid_export_expiry"))?
                .as_secs(),
        )
        .map_err(|_| error("invalid_export_expiry"))?;
        if recipient.trim().is_empty()
            || recipient.len() > 256
            || recipient.chars().any(char::is_control)
            || expires <= now()?
        {
            return Err(error("invalid_export_destination"));
        }
        Ok(Self {
            pool,
            key,
            authorization,
            recipient: Zeroizing::new(recipient),
            expires,
        })
    }
    /// Owner-run migration; grant SELECT/INSERT only to a restricted tenant-RLS
    /// runtime role. Existing deliveries are immutable; no UPDATE or DELETE.
    #[must_use]
    pub const fn migration_sql() -> &'static str {
        "CREATE TABLE forklaunch_export_deliveries(tenant_id uuid NOT NULL,operation_id uuid NOT NULL,batch_id text NOT NULL,recipient_tag text NOT NULL,payload_tag text NOT NULL,expires_epoch bigint NOT NULL,ciphertext text NOT NULL,PRIMARY KEY(tenant_id,operation_id,batch_id));ALTER TABLE forklaunch_export_deliveries ENABLE ROW LEVEL SECURITY;ALTER TABLE forklaunch_export_deliveries FORCE ROW LEVEL SECURITY;CREATE POLICY export_deliveries_tenant ON forklaunch_export_deliveries USING(tenant_id::text=current_setting('forklaunch.tenant_id',true)) WITH CHECK(tenant_id::text=current_setting('forklaunch.tenant_id',true));"
    }
    /// Read an acknowledged batch only after current recipient authorization,
    /// expiry and authenticated content/scope checks. The returned JSON array
    /// can be decoded into the service's typed export DTOs.
    /// # Errors
    /// Rejects revoked authorization, expired artifacts, substitution or missing keys.
    pub async fn read(
        &self,
        request: &SubjectRequest<'_>,
        batch_id: &str,
    ) -> FrameworkResult<ExportDocument> {
        let (tenant, operation) = scope(request)?;
        page(batch_id)?;
        self.authorization
            .authorize(request, Some(&self.recipient), ExportAction::ReadDelivery)
            .await?;
        let mut tx = transaction(&self.pool, request).await?;
        let row=sqlx::query("SELECT recipient_tag,payload_tag,expires_epoch,ciphertext FROM forklaunch_export_deliveries WHERE tenant_id=$1::uuid AND operation_id=$2::uuid AND batch_id=$3")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(batch_id).fetch_one(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        self.authenticate(request, batch_id, &row, false)
    }
    /// Physically delete a bounded set of expired encrypted mailbox rows after
    /// explicit authorization and durable intent. Use a separate cleanup role
    /// with SELECT/UPDATE/DELETE; ordinary delivery roles retain INSERT/SELECT.
    /// This does not purge source snapshots, backups or external destinations.
    /// Expiry and recipient bindings are authenticated before any deletion;
    /// altered rows fail the entire batch closed. Repeated cleanup is harmless.
    /// # Errors
    /// Rejects failed audit/authorization, tampering, missing keys or storage failure.
    pub async fn purge_expired(&self, request: &SubjectRequest<'_>) -> FrameworkResult<usize> {
        let (tenant, operation) = scope(request)?;
        self.authorization
            .authorize(
                request,
                Some(&self.recipient),
                ExportAction::PurgeExpiredDelivery,
            )
            .await?;
        let mut tx = transaction(&self.pool, request).await?;
        let rows = sqlx::query("SELECT batch_id,recipient_tag,payload_tag,expires_epoch,ciphertext FROM forklaunch_export_deliveries WHERE tenant_id=$1::uuid AND operation_id=$2::uuid AND expires_epoch<=$3 ORDER BY batch_id LIMIT $4 FOR UPDATE")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(now()?).bind(i64::try_from(request.batch_size).map_err(|_| error("invalid_export_scope"))?).fetch_all(&mut *tx).await.map_err(storage)?;
        for row in &rows {
            let batch: String = row.try_get("batch_id").map_err(storage)?;
            let _authenticated = self.authenticate(request, &batch, row, true)?;
            sqlx::query("DELETE FROM forklaunch_export_deliveries WHERE tenant_id=$1::uuid AND operation_id=$2::uuid AND batch_id=$3")
                .bind(tenant.to_string()).bind(operation.to_string()).bind(batch).execute(&mut *tx).await.map_err(storage)?;
        }
        tx.commit().await.map_err(storage)?;
        Ok(rows.len())
    }
    fn authenticate(
        &self,
        request: &SubjectRequest<'_>,
        batch_id: &str,
        row: &sqlx::postgres::PgRow,
        allow_expired: bool,
    ) -> FrameworkResult<ExportDocument> {
        let (tenant, operation) = scope(request)?;
        let recipient_tag: String = row.try_get("recipient_tag").map_err(storage)?;
        self.key
            .verify_public_metadata(
                tenant,
                operation,
                "subject-export-recipient",
                &binding(request, Some(&self.recipient))?,
                &recipient_tag,
            )
            .map_err(|_| error("export_recipient_mismatch"))?;
        let expires: i64 = row.try_get("expires_epoch").map_err(storage)?;
        if !allow_expired && expires <= now()? {
            return Err(error("export_delivery_expired"));
        }
        let ciphertext: String = row.try_get("ciphertext").map_err(storage)?;
        let bytes = self
            .key
            .open(tenant, operation, "subject-export-delivery", &ciphertext)
            .map_err(|_| error("invalid_export_delivery"))?;
        let tag: String = row.try_get("payload_tag").map_err(storage)?;
        let signed = delivery_binding(batch_id, &recipient_tag, expires, &bytes)?;
        self.key
            .verify_public_metadata(tenant, operation, "subject-export-content", &signed, &tag)
            .map_err(|_| error("invalid_export_delivery"))?;
        Ok(ExportDocument(bytes))
    }
}
impl SubjectExportSink<ExportDocument> for PostgresExportMailbox {
    async fn deliver(
        &self,
        request: &SubjectRequest<'_>,
        batch: &ExportBatch<ExportDocument>,
    ) -> FrameworkResult<()> {
        let (tenant, operation) = scope(request)?;
        let (start, end) = page(&batch.batch_id)?;
        if batch.records.len()
            != usize::try_from(end - start).map_err(|_| error("invalid_export_page"))?
            || batch.records.len() > request.batch_size
            || self.expires <= now()?
        {
            return Err(error("invalid_export_delivery"));
        }
        self.authorization
            .authorize(request, Some(&self.recipient), ExportAction::Deliver)
            .await?;
        let mut bytes = Zeroizing::new(Vec::new());
        let mut writer = Bounded(&mut bytes, 1_048_576);
        writer
            .write_all(b"[")
            .map_err(|_| error("export_delivery_too_large"))?;
        for (index, document) in batch.records.iter().enumerate() {
            if index > 0 {
                writer
                    .write_all(b",")
                    .map_err(|_| error("export_delivery_too_large"))?;
            }
            writer
                .write_all(document.json_bytes())
                .map_err(|_| error("export_delivery_too_large"))?;
        }
        writer
            .write_all(b"]")
            .map_err(|_| error("export_delivery_too_large"))?;
        let recipient_tag = self
            .key
            .sign_public_metadata(
                tenant,
                operation,
                "subject-export-recipient",
                &binding(request, Some(&self.recipient))?,
            )
            .map_err(|_| error("export_key_unavailable"))?;
        let payload_tag = self
            .key
            .sign_public_metadata(
                tenant,
                operation,
                "subject-export-content",
                &delivery_binding(&batch.batch_id, &recipient_tag, self.expires, &bytes)?,
            )
            .map_err(|_| error("export_key_unavailable"))?;
        let ciphertext = self
            .key
            .seal(tenant, operation, "subject-export-delivery", &bytes)
            .map_err(|_| error("export_key_unavailable"))?;
        let mut tx = transaction(&self.pool, request).await?;
        sqlx::query("INSERT INTO forklaunch_export_deliveries(tenant_id,operation_id,batch_id,recipient_tag,payload_tag,expires_epoch,ciphertext) VALUES($1::uuid,$2::uuid,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(&batch.batch_id).bind(recipient_tag).bind(payload_tag).bind(self.expires).bind(ciphertext).execute(&mut *tx).await.map_err(storage)?;
        let row=sqlx::query("SELECT recipient_tag,payload_tag,expires_epoch,ciphertext FROM forklaunch_export_deliveries WHERE tenant_id=$1::uuid AND operation_id=$2::uuid AND batch_id=$3")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(&batch.batch_id).fetch_one(&mut *tx).await.map_err(storage)?;
        let stored = self.authenticate(request, &batch.batch_id, &row, false)?;
        if stored.json_bytes() != bytes.as_slice()
            || row.try_get::<i64, _>("expires_epoch").map_err(storage)? != self.expires
        {
            return Err(error("export_delivery_changed"));
        }
        tx.commit().await.map_err(storage)
    }
}
fn now() -> FrameworkResult<i64> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| error("invalid_export_clock"))?
            .as_secs(),
    )
    .map_err(|_| error("invalid_export_clock"))
}
fn delivery_binding(
    batch_id: &str,
    recipient_tag: &str,
    expires: i64,
    bytes: &[u8],
) -> FrameworkResult<Zeroizing<Vec<u8>>> {
    let mut binding = Zeroizing::new(
        serde_json::to_vec(&(batch_id, recipient_tag, expires))
            .map_err(|_| error("invalid_export_delivery"))?,
    );
    binding.push(0);
    binding.extend_from_slice(bytes);
    Ok(binding)
}

# Protected persistence and subject lifecycle

These APIs provide explicit storage operations, not a compliance certification.
Enable `encryption` for protected fields; enable `postgres` and `encryption` for
the concrete SQL adapters below. `postgres` adapters use SQLx 0.8 pools. Service
code owns verified actors, subject relationships, lawful holds, database grants,
retention deadlines, external object versions and destination authorization.
Do not infer these policies from an email address or compliance classification.

## Database connections

`connect_runtime_postgres` checks every physical connection for superuser,
RLS-bypass and role-creation privileges for both `session_user` and `current_user`,
including reachable privileged roles. A privileged login cannot evade admission
by setting a restricted active role; `RESET ROLE` would restore its authority.
Use a separate migration owner with `connect_migration_postgres`; runtime
credentials must not own privileged roles. Service migrations still declare
forced row-level security and grants for their tables.

Remote connections require `sslmode=verify-full`. The generated local Docker
configuration explicitly sets `DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT=true` for
its isolated development database network. Leave that exception unset for
remote databases. Connection errors omit credentials and connection strings.

Enable `postgres` for SQLx 0.8 and import pool helpers from `persistence`.
Enable `postgres09` for SQLx 0.9/Apalis and import the same helpers from
`persistence::postgres09`. Both share connection policy and transaction-local
`forklaunch.tenant_id` setup. The concrete subject-lifecycle adapters documented
below use SQLx 0.8; enabling `postgres09` does not convert their pool types.

`clone_runtime_postgres_pool` creates a separate bounded pool for durable audit
writes, so an application transaction cannot starve its own audit connection.
It applies the same transport and role checks to that pool. The transaction
helper scopes `forklaunch.tenant_id`; schemas using a different setting must
explicitly establish their own transaction-local scope.

## Protected values and partial disclosure

`ComplianceMetadata` requires explicit member classifications (`none`, `pii`,
`phi`, `pci`). It describes storage policy and emits build artifacts; it does not
encrypt a field. Use `ProtectedField` for the actual encryption and audited reads.

```rust,ignore
use forklaunch_core::persistence::{
    compliance::ComplianceLevel,
    protected::{DisclosurePolicy, ProtectedField, ProtectedMember},
};

let member = ProtectedMember::new("Contact", "phone", "contact-phone", ComplianceLevel::Pii)?
    .with_disclosure(DisclosurePolicy::Last4);
let field = ProtectedField::seal(&keys, trusted_context, member, phone.as_bytes())?;
store_ciphertext(field.ciphertext()).await?;

// Explicit opt-in projection, authenticated without decrypting the stored value.
let selected_projection = field.disclosed(&keys, trusted_context)?;
// Always returns irreversible "[redacted]", even when a projection was configured.
let anonymous = field.anonymous();
// Actual plaintext access requires an acknowledged audit sink and verified actor.
let plaintext = field.reveal_async(&keys, trusted_context, actor, "contact-read", &audit).await?;
```

`Redact` is the default. `RandomToken` stores an independent random token; it is
not an equality index or a reversible tokenization service. `Year`, `Zip3` and
`Last4` are deliberate partial disclosure, not a legal anonymity guarantee. The
ZIP suppression list provides compatibility with the existing TS strategy, not
an assertion of current geographic privacy policy. Debug output stays redacted;
ordinary protected-field serialization never emits plaintext or a projection.

Existing ciphertext needs an explicit audited reclassification migration before
an opt-in projection becomes available. `protected::migration` prepares audited
rotation/reclassification and applies it through a compare-and-swap storage
boundary; rotation preserves an authenticated random token. Keep required prior
keys until stored fields and unfinished lifecycle operations have migrated.

## Stable export with explicit retention

`subject::export::PostgresExportSnapshot` materializes a bounded encrypted set.
Capture authorized projections consistently from your source transaction first;
the framework does not guess joins or subject ownership. Its authorization trait
must acknowledge durable intent and bind the current verified actor/recipient.

```rust,ignore
use forklaunch_core::persistence::subject::{export::*, export_subject};

let source = PostgresExportSnapshot::new(pool.clone(), keys.clone(), authorization.clone());
// An absolute policy deadline is required. Retries cannot extend it.
source.prepare(&request, &authorized_records, snapshot_expires_at).await?;
let destination = PostgresExportMailbox::new(
    pool.clone(), keys.clone(), authorization.clone(), recipient, delivery_expires_at,
)?;
let progress = export_subject(&source, &destination, &request, 20).await?;
if !progress.complete {
    schedule_retry(request.operation_id).await?;
}
```

Persist the operation UUID and both absolute deadlines across restarts. Reuse the
same scope, batch size and materialized content. Delivery is acknowledged before
source progress; repeating the same batch is idempotent. A changed recipient,
content, expiry, scope or batch size is rejected. Mailbox reads recheck current
authorization and authenticated expiry. Buffers are wiped on drop; decoding into
application DTOs transfers ownership of plaintext to the application.

Owner-run DDL is available from `migration_sql()`. Use forced tenant RLS and
restricted runtime roles. Source records need SELECT/INSERT, snapshot progress
needs SELECT/INSERT/UPDATE, and deliveries need SELECT/INSERT. Cleanup uses a
separate role with DELETE and required locking privileges, never the ordinary
read/delivery role.

```rust,ignore
// Invoke using authorized cleanup adapters with separately granted privileges.
let page = snapshot_cleanup.purge_expired(&request).await?;
if !page.complete { schedule_cleanup_retry(request.operation_id).await?; }
let deliveries_removed = mailbox_cleanup.purge_expired(&request).await?;
```

Cleanup is bounded by the original batch size, audits intent before mutation,
authenticates deadlines and scope, and atomically commits each deletion page.
Expired snapshots deny reads and acknowledgments even before cleanup runs.
Partial source cleanup marks the operation retired; recapture remains denied.
The source tombstone contains keyed scope/content/expiry tags and progress, not
subject identifiers or projection plaintext. Retain tombstones for the entire
operation-ID replay lifetime—indefinitely if old IDs can still be replayed. No
public API deletes them. Receipt deletion is not permission to reuse an operation
ID. Schedule cleanup explicitly; there is no hidden background task or automatic
purging of mailboxes, snapshots, backups or third-party copies together.

## Erasure and external reconciliation

`PostgresSubjectEraseStore::database_only` deletes declared SQL-owned records in
bounded batches, with audited intent, immutable operation cutoff, tenant scope,
legal-hold checks and atomic progress. Its name deliberately excludes external
objects, caches, broker payloads and backups.

For external dependencies, `subject::outbox::PostgresErasureOutbox` seals an
explicit target set in the **same transaction** as the service's source deletion:

```rust,ignore
let mut tx = begin_tenant_transaction(&pool, request.tenant).await?;
outbox.enqueue(&mut tx, &request, &versioned_targets).await?;
delete_authorized_source_rows(&mut tx).await?;
tx.commit().await?;

let result = outbox.reconcile(&request, &object_destination).await?;
if !result.complete { schedule_reconciliation_retry(request.operation_id).await?; }
```

Do not combine `database_only` erasure with an unrelated later outbox insert: that
would leave a crash window. The caller-owned source mutation above must enforce
holds and consistent ownership in the same transaction. Failed enqueue requires
rolling back the transaction. Each `ErasureTarget` has a stable UUID and encrypted
opaque locator. Select immutable object versions or perform conditional deletion
to preserve later writes. A destination must authorize the exact target and return
success only after durable deletion or verified prior deletion; it must tolerate
repeated/concurrent calls with the same ID. No SQL lock spans remote calls.

A remote failure or local receipt failure leaves durable work pending. `complete`
means all targets in this sealed set were acknowledged, not that every copy of a
subject's data has been discovered or deleted. Keep receipt tombstones for the
operation replay lifetime. Actual destination implementations and backup policies
are application-owned boundaries.

## Verification receipts

The tests exercise actual PostgreSQL with forced RLS and restricted roles:

- `subject_export_postgres.rs`: stable replay after delivery/ack failure, revoked
  authorization, cross-tenant denial, ciphertext substitution, encrypted storage,
  authenticated expiry, bounded cleanup rollback, tombstones and recreation denial.
- `subject_outbox_postgres.rs`: source/outbox atomic rollback, changed-set denial,
  remote failure, receipt failure, restart replay, tenant isolation and no false
  completion while targets remain pending.
- `subject_postgres.rs`: SQL erasure cutoff, holds, audit failure, concurrency and retry.
- `protected_disclosure.rs` and `protected_migration.rs`: projection tampering,
  tenant/member binding, random-token preservation, audited migration and rotation.

Database tests are ignored by default and require their explicitly named
disposable loopback database. Read their guards before running; never aim the
fixtures at an application database. Ordinary core tests and strict linting:

```sh
cargo +stable test -p forklaunch-core --features encryption,postgres
cargo +stable clippy -p forklaunch-core --features encryption,postgres --lib -- -D warnings
```

`forklaunch compliance validate-artifacts` validates declared mappings only. A
passing report does not certify that these runtime adapters have been configured,
that a cleanup scheduler runs, or that subject erasure across all stores completed.

### Protected persisted-model bindings

A declaration-only compliance struct remains useful for external storage inventories.
To bind real protected columns, opt in on the actual row type:

```rust,ignore
#[derive(ComplianceMetadata)]
#[compliance(entity = "Note", persisted)]
struct StoredNote {
    #[compliance(level = "none")]
    id: String,
    #[compliance(level = "pii", purpose = "note-title")]
    title: ProtectedField,
}
let member = StoredNote::protected_title()?;
// Seal or load with this member and an explicit trusted tenant/resource context.
row.validate_protected_storage()?;
```

Classified fields must be `ProtectedField` or `Option<ProtectedField>` (aliases work),
not plaintext strings. Generated accessors resolve the single declared purpose;
`ProtectedModel::validate_protected_storage` rejects cross-member policy substitution
before adapters export ciphertext. It does not authenticate stored ciphertext or
authorize reads; reveal still requires explicit context and acknowledged access audit.
Artifacts distinguish `protected-model` bindings from `declarations-only` metadata,
while runtime enforcement still requires independent evidence. Composite purposes
that bind an attachment's parent or a cache version remain explicit audited mappings;
this fixed-purpose derive must not replace or discard those suffixes.

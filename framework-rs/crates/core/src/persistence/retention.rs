//! Bounded, explicitly tenant-scoped retention enforcement hooks.
//!
//! UTC declared-duration policies are available with `retention-calendar`.
//! Other timezone policies require an explicitly resolved cutoff.
//! Stores implement atomic, tenant-bound counting and mutation; this module
//! neither infers compliance from encryption nor performs unscoped table scans.
#[cfg(feature = "retention-calendar")]
mod calendar;
#[cfg(feature = "retention-calendar")]
pub use calendar::RetentionDuration;

use std::{collections::BTreeMap, future::Future, time::SystemTime};

use super::{TenantContext, compliance::ComplianceMetadata};
use crate::common::{FrameworkError, FrameworkResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionAction {
    Delete,
    Anonymize,
}

/// An application-resolved policy. The cutoff is exclusive, matching TS `$lt`.
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    entity: &'static str,
    cutoff: SystemTime,
    action: RetentionAction,
    fields: Vec<&'static str>,
}

impl RetentionPolicy {
    /// Select only explicitly classified, nullable storage fields.
    ///
    /// # Errors
    /// Rejects empty/duplicate storage names and invalid entity metadata.
    pub fn for_entity<T: ComplianceMetadata>(
        cutoff: SystemTime,
        action: RetentionAction,
    ) -> FrameworkResult<Self> {
        let valid = |name: &str| !name.trim().is_empty() && !name.chars().any(char::is_control);
        let mut names = std::collections::BTreeSet::new();
        if !valid(T::ENTITY)
            || T::FIELDS
                .iter()
                .any(|field| !valid(field.storage_name) || !names.insert(field.storage_name))
        {
            return Err(invalid("Invalid compliance metadata for retention"));
        }
        Ok(Self {
            entity: T::ENTITY,
            cutoff,
            action,
            fields: T::FIELDS
                .iter()
                .filter(|field| field.can_anonymize())
                .map(|field| field.storage_name)
                .collect(),
        })
    }

    #[must_use]
    pub fn entity(&self) -> &str {
        self.entity
    }
    #[must_use]
    pub const fn cutoff(&self) -> SystemTime {
        self.cutoff
    }
    #[must_use]
    pub const fn action(&self) -> RetentionAction {
        self.action
    }
    /// Fields that an anonymization adapter may clear to null. An empty list
    /// still requires recording the anonymization timestamp to ensure progress.
    #[must_use]
    pub fn anonymize_fields(&self) -> &[&'static str] {
        &self.fields
    }
    /// Adapters apply this predicate in storage and recheck it atomically when
    /// writing. Anonymized records remain eligible for a later delete policy.
    #[must_use]
    pub fn eligible(&self, created_at: SystemTime, already_anonymized: bool) -> bool {
        created_at < self.cutoff && (self.action == RetentionAction::Delete || !already_anonymized)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RetentionRequest<'a> {
    pub tenant: &'a TenantContext,
    pub policy: &'a RetentionPolicy,
    /// Timestamp to record when anonymizing, fixed for the entire run.
    pub now: SystemTime,
    pub batch_size: usize,
}

/// Application database integration boundary.
///
/// Both methods must constrain queries by `request.tenant`, validate policy
/// fields against storage metadata, and apply the policy's exclusive cutoff.
/// `apply_batch` must select oldest eligible rows first, mutate at most
/// `batch_size` rows atomically, and return only the committed row count.
/// Anonymization nulls only `anonymize_fields` and records its timestamp, including
/// when the field list is empty. Failed/cancelled batches must roll back. Bind
/// tenant encryption context inside the transaction when encrypted fields exist.
pub trait RetentionStore: Sync {
    fn count(
        &self,
        request: RetentionRequest<'_>,
    ) -> impl Future<Output = FrameworkResult<u64>> + Send;
    fn apply_batch(
        &self,
        request: RetentionRequest<'_>,
    ) -> impl Future<Output = FrameworkResult<usize>> + Send;
}

#[derive(Debug, Clone)]
pub struct EnforcementOptions {
    pub dry_run: bool,
    pub batch_size: usize,
    /// Per entity and tenant. Hitting the bound reports incomplete enforcement.
    pub max_batches: usize,
    pub entities: Option<Vec<String>>,
}
impl Default for EnforcementOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            batch_size: 1000,
            max_batches: 1000,
            entities: None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EntityCounts {
    pub deleted: u64,
    pub anonymized: u64,
    /// Failed or incomplete entity/tenant passes. Committed counts are retained.
    pub errors: u64,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EnforcementResult {
    pub processed: u64,
    pub deleted: u64,
    pub anonymized: u64,
    pub errors: u64,
    pub by_entity: BTreeMap<String, EntityCounts>,
}

/// Enforce each selected policy once per explicitly supplied tenant.
/// A store failure stops that entity/tenant pass, then processing continues.
/// Dry runs call only `count`; their totals describe prospective mutations.
///
/// # Errors
/// Rejects invalid bounds, duplicate entities/tenants, empty tenant lists, future
/// cutoffs and counter overflow. Store failures are represented in the result.
pub async fn enforce<S: RetentionStore>(
    store: &S,
    tenants: &[TenantContext],
    policies: &[RetentionPolicy],
    now: SystemTime,
    options: &EnforcementOptions,
) -> FrameworkResult<EnforcementResult> {
    validate(tenants, policies, now, options)?;
    let mut result = EnforcementResult::default();
    for tenant in tenants {
        for policy in policies {
            if options
                .entities
                .as_ref()
                .is_some_and(|names| !names.iter().any(|name| name == policy.entity()))
            {
                continue;
            }
            let counts = result.by_entity.entry(policy.entity().into()).or_default();
            let request = RetentionRequest {
                tenant,
                policy,
                now,
                batch_size: options.batch_size,
            };
            if options.dry_run {
                match store.count(request).await {
                    Ok(count) => add_count(counts, policy.action(), count)?,
                    Err(_) => counts.errors += 1,
                }
                continue;
            }
            let mut exhausted = false;
            for _ in 0..options.max_batches {
                match store.apply_batch(request).await {
                    Ok(0) => {
                        exhausted = true;
                        break;
                    }
                    Ok(count) if count <= options.batch_size => {
                        add_count(counts, policy.action(), count as u64)?;
                    }
                    _ => {
                        counts.errors += 1;
                        exhausted = true;
                        break;
                    }
                }
            }
            if !exhausted {
                // A full final batch may have completed the policy. A bounded
                // count distinguishes completion from a stalled/long-running pass.
                if !matches!(store.count(request).await, Ok(0)) {
                    counts.errors += 1;
                }
            }
        }
    }
    for counts in result.by_entity.values() {
        checked_add(&mut result.deleted, counts.deleted)?;
        checked_add(&mut result.anonymized, counts.anonymized)?;
        checked_add(&mut result.errors, counts.errors)?;
    }
    result.processed = result
        .deleted
        .checked_add(result.anonymized)
        .ok_or_else(|| invalid("Retention count overflow"))?;
    Ok(result)
}

fn validate(
    tenants: &[TenantContext],
    policies: &[RetentionPolicy],
    now: SystemTime,
    options: &EnforcementOptions,
) -> FrameworkResult<()> {
    if tenants.is_empty()
        || tenants.len() > 1024
        || policies.len() > 1024
        || !(1..=10_000).contains(&options.batch_size)
        || !(1..=10_000).contains(&options.max_batches)
    {
        return Err(invalid("Invalid retention scope or batch bounds"));
    }
    let mut tenant_names = std::collections::BTreeSet::new();
    let mut entities = std::collections::BTreeSet::new();
    if tenants
        .iter()
        .any(|tenant| !tenant_names.insert(tenant.id()))
        || policies
            .iter()
            .any(|policy| policy.cutoff > now || !entities.insert(policy.entity()))
    {
        return Err(invalid("Duplicate retention scopes or future cutoff"));
    }
    Ok(())
}
fn add_count(
    counts: &mut EntityCounts,
    action: RetentionAction,
    count: u64,
) -> FrameworkResult<()> {
    checked_add(
        match action {
            RetentionAction::Delete => &mut counts.deleted,
            RetentionAction::Anonymize => &mut counts.anonymized,
        },
        count,
    )
}
fn checked_add(total: &mut u64, count: u64) -> FrameworkResult<()> {
    *total = total
        .checked_add(count)
        .ok_or_else(|| invalid("Retention count overflow"))?;
    Ok(())
}
fn invalid(message: &str) -> FrameworkError {
    FrameworkError::new("retention_invalid", message)
}

#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "postgres")]
pub use postgres::{PostgresRetentionStore, RetentionAudit, RetentionColumns};

/// `SQLx` 0.9 implementation sharing the same retention policy and SQL.
#[cfg(feature = "postgres09")]
pub mod postgres09;

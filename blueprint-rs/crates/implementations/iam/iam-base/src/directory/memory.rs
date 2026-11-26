//! Transactional reference backend. Production stores implement the same boundary.
use super::{
    DirectoryKind, DirectoryStore, DirectoryTransaction, conflict, invalid, not_found, record_id,
    storage_error,
};
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Clone, Default)]
struct State {
    rows: BTreeMap<(DirectoryKind, String), (u64, Value)>,
    sequence: u64,
}

/// Construct one store per trusted tenant. Clones share the same atomic state.
#[derive(Clone, Default)]
pub struct MemoryDirectoryStore {
    state: Arc<Mutex<State>>,
}
pub struct MemoryDirectoryTransaction {
    guard: OwnedMutexGuard<State>,
    pending: State,
    write: bool,
}
impl DirectoryStore for MemoryDirectoryStore {
    type Transaction<'a> = MemoryDirectoryTransaction;
    async fn begin(&self, write: bool) -> BlueprintResult<Self::Transaction<'_>> {
        let guard = self.state.clone().lock_owned().await;
        let pending = guard.clone();
        Ok(MemoryDirectoryTransaction {
            guard,
            pending,
            write,
        })
    }
}
impl MemoryDirectoryTransaction {
    fn require_write(&self) -> BlueprintResult<()> {
        if self.write { Ok(()) } else { Err(invalid()) }
    }
    fn unique(&self, kind: DirectoryKind, value: &Value) -> BlueprintResult<()> {
        let id = record_id(value)?;
        for field in match kind {
            DirectoryKind::User => &["email", "subscription"][..],
            DirectoryKind::Organization => &["subscription"][..],
            _ => &[],
        } {
            if let Some(candidate) = value.get(*field).filter(|value| !value.is_null())
                && self.pending.rows.iter().any(|((family, key), (_, row))| {
                    *family == kind && key != id && row.get(*field) == Some(candidate)
                })
            {
                return Err(conflict());
            }
        }
        Ok(())
    }
}
impl DirectoryTransaction for MemoryDirectoryTransaction {
    async fn get(&mut self, kind: DirectoryKind, id: &str) -> BlueprintResult<Option<Value>> {
        Ok(self
            .pending
            .rows
            .get(&(kind, id.to_owned()))
            .map(|(_, value)| value.clone()))
    }
    async fn list(
        &mut self,
        kind: DirectoryKind,
        ids: Option<&[String]>,
        organization: Option<&str>,
    ) -> BlueprintResult<Vec<Value>> {
        let ids = ids.map(|values| values.iter().map(String::as_str).collect::<BTreeSet<_>>());
        let mut rows = self
            .pending
            .rows
            .iter()
            .filter(|((family, id), (_, value))| {
                *family == kind
                    && ids.as_ref().is_none_or(|ids| ids.contains(id.as_str()))
                    && organization.is_none_or(|organization| {
                        value.get("organization").and_then(Value::as_str) == Some(organization)
                    })
            })
            .map(|(_, (sequence, value))| (*sequence, value.clone()))
            .collect::<Vec<_>>();
        if rows.len() > 4096 {
            return Err(super::scan_limit_error());
        }
        rows.sort_by_key(|(sequence, _)| *sequence);
        Ok(rows.into_iter().map(|(_, value)| value).collect())
    }
    async fn insert(&mut self, kind: DirectoryKind, value: Value) -> BlueprintResult<()> {
        self.require_write()?;
        let key = (kind, record_id(&value)?.to_owned());
        if self.pending.rows.contains_key(&key) {
            return Err(conflict());
        }
        self.unique(kind, &value)?;
        self.pending.sequence = self
            .pending
            .sequence
            .checked_add(1)
            .ok_or_else(storage_error)?;
        self.pending
            .rows
            .insert(key, (self.pending.sequence, value));
        Ok(())
    }
    async fn replace(&mut self, kind: DirectoryKind, value: Value) -> BlueprintResult<()> {
        self.require_write()?;
        let key = (kind, record_id(&value)?.to_owned());
        let sequence = self.pending.rows.get(&key).ok_or_else(not_found)?.0;
        self.unique(kind, &value)?;
        self.pending.rows.insert(key, (sequence, value));
        Ok(())
    }
    async fn delete(&mut self, kind: DirectoryKind, id: &str) -> BlueprintResult<()> {
        self.require_write()?;
        self.pending.rows.remove(&(kind, id.to_owned()));
        for ((family, _), (_, value)) in &mut self.pending.rows {
            let field = match (kind, *family) {
                (DirectoryKind::Role, DirectoryKind::User) => Some("roles"),
                (DirectoryKind::Permission, DirectoryKind::Role) => Some("permissions"),
                _ => None,
            };
            if let Some(field) = field {
                value[field]
                    .as_array_mut()
                    .ok_or_else(storage_error)?
                    .retain(|value| value.as_str() != Some(id));
            }
            if kind == DirectoryKind::Organization
                && *family == DirectoryKind::User
                && value["organization"].as_str() == Some(id)
            {
                value
                    .as_object_mut()
                    .ok_or_else(storage_error)?
                    .remove("organization");
            }
        }
        Ok(())
    }
    async fn commit(mut self) -> BlueprintResult<()> {
        *self.guard = std::mem::take(&mut self.pending);
        Ok(())
    }
}

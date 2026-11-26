//! Application-selected directory compliance, independent of encryption settings.
use super::{DirectoryKind, DirectoryStore, DirectoryTransaction, invalid, record_id};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::compliance::{ComplianceLevel, ComplianceMetadata};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone)]
struct Entity {
    kind: DirectoryKind,
    name: &'static str,
    subject_field: String,
    fields: Vec<&'static str>,
}
/// No entity is classified unless the application registers it explicitly.
#[derive(Debug, Clone, Default)]
pub struct DirectoryCompliance {
    entities: Vec<Entity>,
}
impl DirectoryCompliance {
    /// Bind one directory family to generic compliance metadata. `subject_field`
    /// must be a declared storage field containing a string subject ID.
    ///
    /// # Errors
    /// Rejects duplicate entity/kind registrations, malformed metadata and unknown
    /// subject fields. Encryption configuration is never consulted.
    pub fn with_entity<T: ComplianceMetadata>(
        mut self,
        kind: DirectoryKind,
        subject_field: &str,
    ) -> BlueprintResult<Self> {
        let valid = |name: &str| {
            !name.trim().is_empty() && name.len() <= 256 && !name.chars().any(char::is_control)
        };
        let mut names = std::collections::BTreeSet::new();
        if !valid(T::ENTITY)
            || T::FIELDS.len() > 1024
            || T::FIELDS
                .iter()
                .any(|field| !valid(field.storage_name) || !names.insert(field.storage_name))
            || !names.contains(subject_field)
            || self
                .entities
                .iter()
                .any(|entity| entity.kind == kind || entity.name == T::ENTITY)
        {
            return Err(invalid());
        }
        self.entities.push(Entity {
            kind,
            name: T::ENTITY,
            subject_field: subject_field.into(),
            fields: T::FIELDS
                .iter()
                .filter(|field| field.level != ComplianceLevel::None)
                .map(|field| field.storage_name)
                .collect(),
        });
        Ok(self)
    }
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EntityFailure {
    pub entity_name: String,
    pub reason: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub user_id: String,
    pub entities: BTreeMap<String, Vec<Value>>,
    pub failures: Vec<EntityFailure>,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EraseResult {
    pub entities_affected: Vec<String>,
    pub records_deleted: usize,
    pub failures: Vec<EntityFailure>,
}
/// Stores are already bound to a trusted tenant. Each entity uses a transaction;
/// failed entities are reported while successful committed work is retained.
/// Directory listing bounds remain enforced by the store. Subject links are
/// explicit scalar IDs; ORM relation inference is not performed here.
pub struct DirectoryComplianceService<S> {
    store: Arc<S>,
    config: DirectoryCompliance,
}
impl<S: DirectoryStore> DirectoryComplianceService<S> {
    #[must_use]
    pub fn new(store: Arc<S>, config: DirectoryCompliance) -> Self {
        Self { store, config }
    }
    /// # Errors
    /// Rejects invalid subject IDs. Storage failures are returned in `failures`,
    /// which callers must treat as an incomplete export.
    pub async fn export(&self, subject: &str) -> BlueprintResult<ExportResult> {
        validate_subject(subject)?;
        let mut result = ExportResult {
            user_id: subject.into(),
            entities: BTreeMap::new(),
            failures: Vec::new(),
        };
        for entity in self
            .config
            .entities
            .iter()
            .filter(|entity| !entity.fields.is_empty())
        {
            match self.export_entity(entity, subject).await {
                Ok(rows) if !rows.is_empty() => {
                    result.entities.insert(entity.name.into(), rows);
                }
                Ok(_) => {}
                Err(error) => result.failures.push(failure(entity, &error)),
            }
        }
        Ok(result)
    }
    async fn export_entity(&self, entity: &Entity, subject: &str) -> BlueprintResult<Vec<Value>> {
        let mut tx = self.store.begin(false).await?;
        let rows = selected(&mut tx, entity, subject).await?;
        let mut output = Vec::new();
        for row in rows {
            let mut fields = Map::new();
            fields.insert("id".into(), Value::String(record_id(&row)?.into()));
            for name in &entity.fields {
                if let Some(value) = row.get(*name) {
                    fields.insert((*name).into(), value.clone());
                }
            }
            output.push(Value::Object(fields));
        }
        tx.commit().await?;
        Ok(output)
    }
    /// Erase whole matching records for explicitly classified entities, as the
    /// TypeScript compliance service does. Counts include only committed deletes.
    ///
    /// # Errors
    /// Rejects invalid subjects. A nonempty `failures` list means incomplete erasure.
    pub async fn erase(&self, subject: &str) -> BlueprintResult<EraseResult> {
        validate_subject(subject)?;
        let mut result = EraseResult::default();
        for entity in self
            .config
            .entities
            .iter()
            .filter(|entity| !entity.fields.is_empty())
        {
            match self.erase_entity(entity, subject).await {
                Ok(count) if count > 0 => {
                    result.records_deleted += count;
                    result.entities_affected.push(entity.name.into());
                }
                Ok(_) => {}
                Err(error) => result.failures.push(failure(entity, &error)),
            }
        }
        Ok(result)
    }
    async fn erase_entity(&self, entity: &Entity, subject: &str) -> BlueprintResult<usize> {
        let mut tx = self.store.begin(true).await?;
        let rows = selected(&mut tx, entity, subject).await?;
        for row in &rows {
            tx.delete(entity.kind, record_id(row)?).await?;
        }
        tx.commit().await?;
        Ok(rows.len())
    }
}
async fn selected<T: DirectoryTransaction>(
    tx: &mut T,
    entity: &Entity,
    subject: &str,
) -> BlueprintResult<Vec<Value>> {
    if entity.subject_field == "id" {
        return Ok(tx.get(entity.kind, subject).await?.into_iter().collect());
    }
    Ok(tx
        .list(entity.kind, None, None)
        .await?
        .into_iter()
        .filter(|row| row.get(&entity.subject_field).and_then(Value::as_str) == Some(subject))
        .collect())
}
fn validate_subject(subject: &str) -> BlueprintResult<()> {
    if subject.trim().is_empty() || subject.len() > 256 || subject.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(())
}
fn failure(entity: &Entity, _error: &BlueprintError) -> EntityFailure {
    EntityFailure {
        entity_name: entity.name.into(),
        reason: "IAM directory storage operation failed".into(),
    }
}

#[cfg(test)]
pub(crate) mod tests;

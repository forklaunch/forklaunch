use super::{
    BillingKind, BillingStore, Scope, conflict, invalid, merge, not_found, storage_error,
    validate_id, validate_record,
};
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
type Key = (Scope, BillingKind, String);
#[derive(Debug)]
struct Entry {
    data: Value,
    expires: Option<Instant>,
    sequence: u64,
}
#[derive(Debug, Default)]
struct State {
    rows: HashMap<Key, Entry>,
    sequence: u64,
}
#[derive(Debug, Default)]
pub struct MemoryBillingStore {
    state: Mutex<State>,
}
impl MemoryBillingStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
impl BillingStore for MemoryBillingStore {
    async fn insert(
        &self,
        scope: Scope,
        kind: BillingKind,
        data: Value,
        ttl: Duration,
    ) -> BlueprintResult<Value> {
        if kind == BillingKind::Customer && scope != Scope::Durable {
            return Err(invalid());
        }
        validate_record(kind, &data)?;
        let id = data["id"].as_str().ok_or_else(invalid)?.to_owned();
        validate_id(&id)?;
        let mut state = self.state.lock().map_err(|_| storage_error())?;
        let now = Instant::now();
        state
            .rows
            .retain(|_, row| row.expires.is_none_or(|expires| expires > now));
        let key = (scope, kind, id);
        if scope == Scope::Durable && state.rows.contains_key(&key) {
            return Err(conflict());
        }
        if scope == Scope::Durable
            && matches!(kind, BillingKind::Plan | BillingKind::Subscription)
            && state.rows.iter().any(|((row_scope, row_kind, _), row)| {
                *row_scope == scope
                    && *row_kind == kind
                    && row.data["externalId"] == data["externalId"]
            })
        {
            return Err(conflict());
        }
        state.sequence = state.sequence.checked_add(1).ok_or_else(storage_error)?;
        let sequence = state.sequence;
        state.rows.insert(
            key,
            Entry {
                data: data.clone(),
                expires: if scope == Scope::Cache {
                    Some(now.checked_add(ttl).ok_or_else(invalid)?)
                } else {
                    None
                },
                sequence,
            },
        );
        Ok(data)
    }
    async fn get(
        &self,
        scope: Scope,
        kind: BillingKind,
        id: &str,
    ) -> BlueprintResult<Option<Value>> {
        let state = self.state.lock().map_err(|_| storage_error())?;
        Ok(state
            .rows
            .get(&(scope, kind, id.to_owned()))
            .filter(|row| row.expires.is_none_or(|expires| expires > Instant::now()))
            .map(|row| row.data.clone()))
    }
    async fn list_configured_cached(
        &self,
        ids: Option<&[String]>,
        limit: usize,
    ) -> BlueprintResult<Vec<(String, Value)>> {
        super::cached_dates::selection(ids, limit)?;
        let state = self.state.lock().map_err(|_| storage_error())?;
        let now = Instant::now();
        let selected = if let Some(ids) = ids.filter(|ids| !ids.is_empty()) {
            ids.iter()
                .map(|id| {
                    let row = state
                        .rows
                        .get(&(Scope::Cache, BillingKind::PaymentLink, id.clone()))
                        .filter(|row| row.expires.is_none_or(|expiry| expiry > now))
                        .ok_or_else(super::cached_dates::list_invalid)?;
                    Ok((id.as_str(), row))
                })
                .collect::<BlueprintResult<Vec<_>>>()?
        } else {
            let mut rows = state
                .rows
                .iter()
                .filter(|((scope, kind, _), row)| {
                    *scope == Scope::Cache
                        && *kind == BillingKind::PaymentLink
                        && row.expires.is_none_or(|expiry| expiry > now)
                })
                .take(limit + 1)
                .map(|((_, _, id), row)| (id.as_str(), row))
                .collect::<Vec<_>>();
            if rows.len() > limit {
                return Err(super::cached_dates::list_invalid());
            }
            rows.sort_by_key(|(_, row)| row.sequence);
            rows
        };
        selected
            .into_iter()
            .map(|(id, row)| {
                Ok((
                    id.to_owned(),
                    super::cached_dates::normalize(BillingKind::PaymentLink, row.data.clone())?,
                ))
            })
            .collect()
    }
    async fn patch(
        &self,
        scope: Scope,
        kind: BillingKind,
        id: &str,
        patch: Value,
        ttl: Duration,
    ) -> BlueprintResult<Value> {
        let mut state = self.state.lock().map_err(|_| storage_error())?;
        let key = (scope, kind, id.to_owned());
        let old = state
            .rows
            .get(&key)
            .filter(|row| row.expires.is_none_or(|expires| expires > Instant::now()))
            .ok_or_else(not_found)?;
        let data = merge(old.data.clone(), &patch)?;
        validate_record(kind, &data)?;
        if data["id"].as_str() != Some(id) {
            return Err(invalid());
        }
        if scope == Scope::Durable
            && matches!(kind, BillingKind::Plan | BillingKind::Subscription)
            && state.rows.iter().any(|(other, row)| {
                other != &key
                    && other.0 == scope
                    && other.1 == kind
                    && row.data["externalId"] == data["externalId"]
            })
        {
            return Err(conflict());
        }
        let row = state.rows.get_mut(&key).ok_or_else(not_found)?;
        row.data = data.clone();
        if scope == Scope::Cache {
            row.expires = Some(Instant::now().checked_add(ttl).ok_or_else(invalid)?);
        }
        Ok(data)
    }
    async fn delete(&self, scope: Scope, kind: BillingKind, id: &str) -> BlueprintResult<bool> {
        let mut state = self.state.lock().map_err(|_| storage_error())?;
        Ok(state
            .rows
            .remove(&(scope, kind, id.to_owned()))
            .is_some_and(|row| row.expires.is_none_or(|expires| expires > Instant::now())))
    }
    async fn list(
        &self,
        scope: Scope,
        kind: BillingKind,
        ids: Option<&[String]>,
    ) -> BlueprintResult<Vec<Value>> {
        let state = self.state.lock().map_err(|_| storage_error())?;
        if scope == Scope::Cache
            && let Some(ids) = ids.filter(|ids| !ids.is_empty())
        {
            return ids
                .iter()
                .map(|id| {
                    state
                        .rows
                        .get(&(scope, kind, id.clone()))
                        .filter(|row| row.expires.is_none_or(|expires| expires > Instant::now()))
                        .map(|row| row.data.clone())
                        .ok_or_else(not_found)
                })
                .collect();
        }
        let mut rows: Vec<_> = state
            .rows
            .iter()
            .filter(|((row_scope, row_kind, id), row)| {
                *row_scope == scope
                    && *row_kind == kind
                    && row.expires.is_none_or(|expires| expires > Instant::now())
                    && ids.is_none_or(|ids| ids.is_empty() || ids.contains(id))
            })
            .map(|(_, row)| row)
            .collect();
        rows.sort_by_key(|row| row.sequence);
        Ok(rows.into_iter().map(|row| row.data.clone()).collect())
    }
    async fn find_active_subscription(
        &self,
        party_id: &str,
        party_type: &str,
    ) -> BlueprintResult<Option<Value>> {
        let state = self.state.lock().map_err(|_| storage_error())?;
        Ok(state
            .rows
            .iter()
            .filter(|((scope, kind, _), row)| {
                *scope == Scope::Durable
                    && *kind == BillingKind::Subscription
                    && row.data["partyId"] == party_id
                    && row.data["partyType"] == party_type
                    && row.data["active"] == true
            })
            .min_by_key(|(_, row)| row.sequence)
            .map(|(_, row)| row.data.clone()))
    }
    async fn find_subscription_id_by_external_id(
        &self,
        external_id: &str,
    ) -> BlueprintResult<Option<String>> {
        let state = self.state.lock().map_err(|_| storage_error())?;
        Ok(state
            .rows
            .iter()
            .find(|((scope, kind, _), row)| {
                *scope == Scope::Durable
                    && *kind == BillingKind::Subscription
                    && row.data["externalId"] == external_id
            })
            .map(|((_, _, id), _)| id.clone()))
    }
}

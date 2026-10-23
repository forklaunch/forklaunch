//! JSON object operations with optional tenant-bound authenticated encryption.
use crate::{AsyncObjectStore, ObjectMetadata};
use forklaunch_core::{
    common::FrameworkResult,
    persistence::{TenantContext, json::JsonRecordCodec},
};
use serde::{Serialize, de::DeserializeOwned};

pub struct JsonObjectStore<S> {
    store: S,
    codec: JsonRecordCodec,
}

pub type JsonObjectRecord<T> = (T, ObjectMetadata);

fn json_metadata(mut metadata: ObjectMetadata) -> ObjectMetadata {
    if metadata.content_type.is_none() {
        metadata.content_type = Some("application/json".into());
    }
    metadata
}

impl<S: AsyncObjectStore> JsonObjectStore<S> {
    #[must_use]
    pub fn new(store: S, codec: JsonRecordCodec) -> Self {
        Self { store, codec }
    }

    #[must_use]
    pub fn into_inner(self) -> S {
        self.store
    }

    /// Leaves file/binary operations on the underlying store untouched.
    /// # Errors
    /// Propagates serialization, encryption and backend errors.
    pub async fn put_object<T: Serialize + ?Sized>(
        &self,
        key: &str,
        value: &T,
        metadata: ObjectMetadata,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<()> {
        let body = self.codec.encode(value, tenant)?;
        self.store.put(key, body, json_metadata(metadata)).await?;
        Ok(())
    }

    /// Returns typed JSON with original content type/tags; misses remain `None`.
    /// # Errors
    /// Propagates backend and decoding/authentication errors, including wrong tenants.
    pub async fn get_object<T: DeserializeOwned>(
        &self,
        key: &str,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Option<(T, ObjectMetadata)>> {
        self.store
            .get(key)
            .await?
            .map(|object| Ok((self.codec.decode(&object.body, tenant)?, object.metadata)))
            .transpose()
    }

    /// Serializes and encrypts every record before making any backing-store writes.
    /// Backend writes are not atomic; earlier writes can survive a later backend failure.
    /// # Errors
    /// Propagates JSON, encryption and backing-store failures.
    pub async fn put_batch_objects<T: Serialize>(
        &self,
        objects: Vec<(String, T, ObjectMetadata)>,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<()> {
        let objects = objects
            .into_iter()
            .map(|(key, value, metadata)| {
                Ok((
                    key,
                    self.codec.encode(&value, tenant)?,
                    json_metadata(metadata),
                ))
            })
            .collect::<FrameworkResult<Vec<_>>>()?;
        self.store.put_batch(objects).await?;
        Ok(())
    }

    /// Keeps input order, missing objects, tags, content types and false/zero/null JSON values.
    /// # Errors
    /// Propagates backend, JSON and authentication failures.
    pub async fn get_batch_objects<T: DeserializeOwned>(
        &self,
        keys: &[String],
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Vec<Option<JsonObjectRecord<T>>>> {
        self.store
            .get_batch(keys)
            .await?
            .into_iter()
            .map(|object| {
                object
                    .map(|object| Ok((self.codec.decode(&object.body, tenant)?, object.metadata)))
                    .transpose()
            })
            .collect()
    }

    /// # Errors
    /// Propagates key validation and backend failures. Existence is advisory under concurrency.
    pub async fn delete_object(&self, key: &str) -> FrameworkResult<bool> {
        self.store.delete(key).await
    }

    /// # Errors
    /// Propagates backend failures; earlier deletions can remain committed on failure.
    pub async fn delete_batch_objects(&self, keys: &[String]) -> FrameworkResult<usize> {
        self.store.delete_batch(keys).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoredObject;
    use forklaunch_core::persistence::encryption::FieldEncryptor;
    use serde::Serializer;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct WriteCounter(AtomicUsize);
    impl AsyncObjectStore for WriteCounter {
        async fn put(
            &self,
            key: &str,
            body: Vec<u8>,
            metadata: ObjectMetadata,
        ) -> FrameworkResult<StoredObject> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(StoredObject {
                key: key.to_owned(),
                body,
                metadata,
            })
        }
        async fn get(&self, _: &str) -> FrameworkResult<Option<StoredObject>> {
            Ok(None)
        }
        async fn delete(&self, _: &str) -> FrameworkResult<bool> {
            Ok(false)
        }
        async fn list(&self, _: &str) -> FrameworkResult<Vec<StoredObject>> {
            Ok(Vec::new())
        }
    }
    struct Fallible(bool);
    impl Serialize for Fallible {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            if self.0 {
                serializer.serialize_bool(false)
            } else {
                Err(serde::ser::Error::custom("secret must never escape"))
            }
        }
    }

    #[tokio::test]
    async fn a_serialization_failure_prevents_every_batch_write_and_redacts_its_details() {
        let store = JsonObjectStore::new(
            WriteCounter::default(),
            JsonRecordCodec::new(FieldEncryptor::new("fixture-key").unwrap()),
        );
        let tenant = TenantContext::new("tenant-a").unwrap();
        let error = store
            .put_batch_objects(
                vec![
                    ("first".into(), Fallible(true), ObjectMetadata::new()),
                    ("second".into(), Fallible(false), ObjectMetadata::new()),
                ],
                Some(&tenant),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), "record_json_invalid");
        assert!(!error.to_string().contains("secret"));
        assert_eq!(store.store.0.load(Ordering::SeqCst), 0);
    }
}

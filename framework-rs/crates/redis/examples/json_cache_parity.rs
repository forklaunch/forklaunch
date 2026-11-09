use forklaunch_core::persistence::{
    TenantContext,
    encryption::{CiphertextFormat, FieldEncryptor},
    json::JsonRecordCodec,
};
use forklaunch_infrastructure_redis::{
    RedisTtlCacheConfig,
    client::{RedisQueue, RedisTtlCache},
    json::{JsonCache, JsonCacheRecord, JsonCacheWrite, JsonQueue},
};
use serde_json::{Value, json};
use std::time::Duration;

fn json_record(record: Option<JsonCacheRecord<Value>>) -> Value {
    match record {
        Some(record) => {
            json!({"key":record.key,"value":record.value,"ttl":record.remaining_ttl.map(|ttl|ttl.as_millis())})
        }
        None => Value::Null,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Value = serde_json::from_reader(std::io::stdin())?;
    let previous: Vec<String> = serde_json::from_value(input["previous"].clone())?;
    let encryptor = FieldEncryptor::new(input["master"].as_str().ok_or("Master required")?)?
        .with_previous_keys(&previous)?
        .with_format(if input["format"] == "v3" {
            CiphertextFormat::V3
        } else {
            CiphertextFormat::V2
        });
    let url = std::env::var("FORKLAUNCH_TEST_REDIS_URL")?;
    let codec = |encryptor| {
        let codec = JsonRecordCodec::new(encryptor);
        if input["typescriptSafe"] == true {
            codec.with_typescript_safe_values()
        } else {
            codec
        }
    };
    let mut queue = input["queue"]
        .as_str()
        .map(|key| RedisQueue::connect(&url, key))
        .transpose()?
        .map(|queue| JsonQueue::new(queue, codec(encryptor.clone())));
    let mut cache = JsonCache::new(
        RedisTtlCache::connect(RedisTtlCacheConfig::new(url, Duration::from_secs(60)))?,
        codec(encryptor),
    );
    let tenant = input["tenant"]
        .as_str()
        .map(TenantContext::new)
        .transpose()?;
    let mut output = Vec::new();
    for operation in input["operations"]
        .as_array()
        .ok_or("Operations required")?
    {
        output.push(execute(operation, &mut cache, &mut queue, tenant.as_ref())?);
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

// Keep the fixture's wire operation dispatch in one table.
#[allow(clippy::too_many_lines)]
fn execute(
    operation: &Value,
    cache: &mut JsonCache<RedisTtlCache>,
    queue: &mut Option<JsonQueue>,
    tenant: Option<&TenantContext>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let key = operation["key"].as_str().unwrap_or("");
    let result = match operation["op"].as_str().ok_or("Operation required")? {
        "peek" => cache.peek_record(key).map(|value| json!(value)),
        "peekBatch" => {
            let keys: Vec<String> = serde_json::from_value(operation["keys"].clone())?;
            cache.peek_batch_records(&keys).map(|values| json!(values))
        }
        "deleteBatch" => {
            let keys: Vec<String> = serde_json::from_value(operation["keys"].clone())?;
            cache.delete_batch_records(&keys).map(|value| json!(value))
        }
        "peekPrefix" => cache
            .peek_records_by_prefix(key)
            .map(|values| json!(values)),
        "keys" => cache.list_keys_by_prefix(key).map(|values| json!(values)),
        "matchingKeys" => cache.list_keys_matching(key).map(|values| json!(values)),
        "ttl" => Ok(json!(cache.backend().config().default_ttl().as_millis())),
        "rawExists" => {
            let present: bool = redis::cmd("EXISTS")
                .arg(key)
                .query(cache.backend_mut().connection_mut())?;
            Ok(json!(present))
        }
        "put" => cache
            .put_record(
                key,
                &operation["value"],
                operation["ttl"].as_u64().map(Duration::from_millis),
                tenant,
            )
            .map(|()| Value::Null),
        "get" => cache
            .read_record::<Value>(key, tenant)
            .map(|value| json!({"value":value,"found":value.is_some()})),
        "getWithTtl" => cache
            .read_record_with_ttl::<Value>(key, tenant)
            .map(json_record),
        "putRecords" => {
            let records = operation["records"]
                .as_array()
                .ok_or("Records required")?
                .iter()
                .map(|record| {
                    Ok(JsonCacheWrite {
                        key: record["key"]
                            .as_str()
                            .ok_or("Record key required")?
                            .to_owned(),
                        value: record["value"].clone(),
                        ttl: record["ttl"].as_u64().map(Duration::from_millis),
                    })
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
            cache.put_records(records, tenant).map(|()| Value::Null)
        }
        "readRecords" => {
            let keys: Vec<String> = serde_json::from_value(operation["keys"].clone())?;
            cache
                .read_records::<Value>(&keys, tenant)
                .map(|records| Value::Array(records.into_iter().map(json_record).collect()))
        }
        "prefix" => cache
            .read_records_by_prefix::<Value>(key, tenant)
            .map(|records| {
                Value::Array(
                    records
                        .into_iter()
                        .map(|record| json_record(Some(record)))
                        .collect(),
                )
            }),
        "queuePut" => queue
            .as_mut()
            .ok_or("Queue required")?
            .enqueue_record(&operation["value"], tenant)
            .map(|()| Value::Null),
        "queuePutBatch" => queue
            .as_mut()
            .ok_or("Queue required")?
            .enqueue_batch_records(
                operation["values"].as_array().ok_or("Values required")?,
                tenant,
            )
            .map(|()| Value::Null),
        "queueGet" => queue
            .as_mut()
            .ok_or("Queue required")?
            .dequeue_record::<Value>(tenant)
            .map(|value| json!({"value":value,"found":value.is_some()})),
        "queueGetBatch" | "queuePeekBatch" => {
            let max = usize::try_from(operation["count"].as_u64().ok_or("Count required")?)?;
            let queue = queue.as_mut().ok_or("Queue required")?;
            let values = if operation["op"] == "queueGetBatch" {
                queue.dequeue_batch_records::<Value>(max, tenant)
            } else {
                queue.peek_records::<Value>(max, tenant)
            };
            values.map(|values| json!(values))
        }
        "queueGetBatchTs" | "queuePeekBatchTs" => {
            let max = usize::try_from(operation["count"].as_u64().ok_or("Count required")?)?;
            let queue = queue.as_mut().ok_or("Queue required")?;
            let values = if operation["op"] == "queueGetBatchTs" {
                queue.dequeue_batch_records_typescript::<Value>(max, tenant)
            } else {
                queue.peek_records_typescript::<Value>(max, tenant)
            };
            values.map(|values| json!(values))
        }
        "queuePeek" => queue
            .as_mut()
            .ok_or("Queue required")?
            .peek_record::<Value>(tenant)
            .map(|value| json!({"value":value,"found":value.is_some()})),
        "batch" => {
            let keys: Vec<String> = serde_json::from_value(operation["keys"].clone())?;
            cache
                .read_batch_records::<Value>(&keys, tenant)
                .map(|values| json!(values))
        }
        _ => return Err("Unknown operation".into()),
    };
    Ok(match result {
        Ok(value) => value,
        Err(error) => json!({"error":error.code()}),
    })
}

use forklaunch_infrastructure_redis::client::{RedisQueue, RedisQueueLayout};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Value = serde_json::from_reader(std::io::stdin())?;
    let url = std::env::var("FORKLAUNCH_TEST_REDIS_URL")?;
    let key = input["key"].as_str().ok_or("Queue key required")?;
    let layout = if input["layout"] == "legacy" {
        RedisQueueLayout::LegacyNative
    } else {
        RedisQueueLayout::TypeScript
    };
    let mut queue = RedisQueue::connect_with_layout(&url, key, layout)?;
    let mut output = Vec::new();
    for operation in input["operations"]
        .as_array()
        .ok_or("Operations required")?
    {
        let name = operation["op"].as_str().ok_or("Operation name required")?;
        let result = match name {
            "enqueue" => {
                queue.enqueue(&serde_json::to_vec(&operation["value"])?)?;
                Value::Null
            }
            "enqueueBatch" => {
                let values = operation["values"]
                    .as_array()
                    .ok_or("Batch values required")?
                    .iter()
                    .map(serde_json::to_vec)
                    .collect::<Result<Vec<_>, _>>()?;
                queue.enqueue_batch(&values)?;
                Value::Null
            }
            "dequeue" => queue
                .dequeue()?
                .as_deref()
                .map(serde_json::from_slice)
                .transpose()?
                .unwrap_or(Value::Null),
            "peek" => queue
                .peek()?
                .as_deref()
                .map(serde_json::from_slice)
                .transpose()?
                .unwrap_or(Value::Null),
            "dequeueBatch" | "peekBatch" => {
                let count = usize::try_from(operation["count"].as_u64().ok_or("Count required")?)?;
                let values = if name == "dequeueBatch" {
                    queue.dequeue_batch(count)?
                } else {
                    queue.peek_batch(count)?
                };
                Value::Array(
                    values
                        .iter()
                        .map(|value| serde_json::from_slice(value))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            }
            "length" => json!(queue.len()?),
            _ => return Err("Unsupported operation".into()),
        };
        output.push(result);
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

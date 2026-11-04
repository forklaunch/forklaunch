use std::io::{self, Read};

use serde_json::{Value, json};

// Replace this sample with the job's assignment. Keep the input/output contract.
// The execution service owns time, memory, network, credentials, and spending limits.
fn execute(input: Value) -> Result<Value, String> {
    let text = input
        .get("text")
        .and_then(Value::as_str)
        .ok_or("Provide text to summarize.")?;
    Ok(json!({"wordCount": text.split_whitespace().count()}))
}
fn main() {
    let mut input = String::new();
    let result = io::stdin()
        .take(1_048_577)
        .read_to_string(&mut input)
        .map_err(|_| "Could not read job input.".to_owned())
        .and_then(|_| {
            if input.len() > 1_048_576 {
                Err("Job input is too large.".into())
            } else {
                Ok(())
            }
        })
        .and_then(|_| {
            serde_json::from_str(&input).map_err(|_| "Provide valid JSON input.".to_owned())
        })
        .and_then(execute);
    match result {
        Ok(output) => println!("{}", output),
        Err(message) => {
            eprintln!("{}", message);
            std::process::exit(1);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_words() {
        assert_eq!(
            execute(json!({"text":"one two three"})).unwrap()["wordCount"],
            3
        );
    }
    #[test]
    fn rejects_missing_text() {
        assert!(execute(json!({})).is_err());
    }
}

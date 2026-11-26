use forklaunch_validator::typescript_coerce::{DateCoercionError, date_epoch_millis};
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let value = case
            .get("raw")
            .and_then(Value::as_str)
            .map(|raw| serde_json::from_str::<Value>(raw).unwrap());
        let result = match date_epoch_millis(value.as_ref()) {
            Ok(millis) => json!({"ok":true,"millis":millis}),
            Err(DateCoercionError::Validation(error)) => json!({"ok":false,"errors":[error]}),
            Err(DateCoercionError::UnsupportedStringFormat) => json!({"unsupported":true}),
        };
        println!("{result}");
    }
}

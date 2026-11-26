//! JSON-line bridge for the actual Node/default Express query oracle.
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Option<String> =
            serde_json::from_str(&line.expect("stdin line")).expect("JSON input");
        let ordered =
            forklaunch_core::http::parse_typescript_simple_query_ordered(input.as_deref());
        println!(
            "{}",
            serde_json::json!({
                "fields": forklaunch_core::http::parse_typescript_simple_query(input.as_deref()),
                "orderedKeys": ordered.keys().collect::<Vec<_>>(),
            })
        );
    }
}

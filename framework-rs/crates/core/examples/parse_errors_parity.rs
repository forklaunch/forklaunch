use forklaunch_core::http::{
    ParseError, pretty_print_parse_errors, typescript_request_validation_error_response,
    typescript_response_validation_error_response,
};
use serde::Deserialize;
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct Case {
    errors: Option<Vec<ParseError>>,
    prefix: Option<String>,
    #[serde(default)]
    request_response: bool,
    correlation_id: Option<String>,
    #[serde(default)]
    response_response: bool,
    header_errors: Option<Vec<ParseError>>,
}

fn main() {
    for line in io::stdin().lock().lines() {
        let case: Case = serde_json::from_str(&line.unwrap()).unwrap();
        if case.response_response {
            let response = typescript_response_validation_error_response(
                case.header_errors.as_deref(),
                case.errors.as_deref(),
                case.correlation_id.as_deref(),
            );
            println!("{}", response.map_or(serde_json::Value::Null, |response| serde_json::json!({"status":response.status_code,"type":response.headers.get("content-type"),"body":response.body.as_text()})));
            continue;
        }
        if case.request_response {
            let response = typescript_request_validation_error_response(
                case.errors.as_deref(),
                case.correlation_id.as_deref(),
            );
            println!(
                "{}",
                serde_json::json!({
                    "status":response.status_code,
                    "type":response.headers.get("content-type"),
                    "body":response.body.as_text(),
                })
            );
            continue;
        }
        println!(
            "{}",
            serde_json::to_string(&pretty_print_parse_errors(
                case.errors.as_deref(),
                case.prefix.as_deref()
            ))
            .unwrap()
        );
    }
}

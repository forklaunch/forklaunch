#[path = "../src/http/request_dates.rs"]
mod request_dates;
use forklaunch_core::contract::{BodyContract, RouteContract};
use request_dates::BillingRequestDateProfile;
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let mut body = BodyContract::json("DateInput");
        body.schema.json_schema = Some(input["schema"].to_string());
        let mut route =
            RouteContract::post("Date input", "Date input", "/dates").with_request_body(body);
        let profile = if input["legacy"] == true {
            BillingRequestDateProfile::Legacy
        } else {
            BillingRequestDateProfile::UtcIsoAndEpoch
        };
        profile.project(&mut route).unwrap();
        let mut value = input["body"].clone();
        let result = profile.normalize(&route, &mut value);
        println!(
            "{}",
            json!({"ok":result.is_ok(),"code":result.err().map(|e|e.code),"value":value,"schema":serde_json::from_str::<Value>(route.request.body.unwrap().schema.json_schema.as_ref().unwrap()).unwrap()})
        );
    }
}

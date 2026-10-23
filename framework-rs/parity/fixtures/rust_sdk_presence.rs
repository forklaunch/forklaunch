use presence_sdk::*;
use serde_json::Value;

#[tokio::main]
async fn main() {
    tokio::time::timeout(std::time::Duration::from_secs(30), verify())
        .await
        .unwrap();
}
async fn verify() {
    let base = std::env::args().nth(1).unwrap();
    let cases: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(std::env::args().nth(2).unwrap()).unwrap())
            .unwrap();
    let client = PresenceClient::new(base);
    for (index, case) in cases.iter().enumerate() {
        let decoded =
            serde_json::from_value::<PresencePresenceReadPresenceResponse>(case["value"].clone());
        assert_eq!(
            decoded.is_ok(),
            case["valid"].as_bool().unwrap(),
            "case {index}: {}",
            case["value"]
        );
        let reply = client
            .presence_read_presence(PresencePresenceReadPresenceRequest {
                params: [("index".into(), index.to_string())].into(),
                ..Default::default()
            })
            .await;
        if case["valid"] == true {
            let original = serde_json::to_value(decoded.unwrap()).unwrap();
            assert_eq!(original, case["normalized"], "roundtrip case {index}");
            let PresencePresenceReadPresenceResponses::Status200(value) = reply.unwrap() else {
                panic!("Presence response missing")
            };
            assert_eq!(serde_json::to_value(&value).unwrap(), original);
            let PresencePresenceWritePresenceResponses::Status200(echo) = client
                .presence_write_presence(PresencePresenceWritePresenceRequest {
                    body: Some(serde_json::from_value(serde_json::to_value(&value).unwrap()).unwrap()),
                    ..Default::default()
                })
                .await
                .unwrap()
            else {
                panic!("Presence echo missing")
            };
            assert_eq!(serde_json::to_value(echo).unwrap(), original);
        } else {
            assert!(reply.is_err(), "invalid case {index}");
        }
    }
    println!(
        "PASS {} Rust SDK cases match actual TypeScript validation and omission/null roundtrips",
        cases.len()
    );
}

use forklaunch_testing::{
    TEST_TOKENS, TestContainerEndpoint, TestEnvConfig, TestService, setup_test_environment,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{self, BufRead},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    #[serde(default)]
    baseline: BTreeMap<String, String>,
    database_type: Option<String>,
    database: Option<Endpoint>,
    redis: Option<Endpoint>,
    kafka: Option<Endpoint>,
    s3: Option<Endpoint>,
    hmac_secret: Option<String>,
    #[serde(default)]
    custom_vars: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Endpoint {
    host: String,
    ports: BTreeMap<u16, u16>,
}
impl From<Endpoint> for TestContainerEndpoint {
    fn from(value: Endpoint) -> Self {
        Self {
            host: value.host,
            mapped_ports: value.ports,
        }
    }
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--echo-child") {
        println!("{}", std::env::var("FORKLAUNCH_ENV_PARITY_CHILD").unwrap());
        return;
    }
    let environment = setup_test_environment(
        &TestEnvConfig {
            custom_vars: BTreeMap::from([(
                "FORKLAUNCH_ENV_PARITY_CHILD".into(),
                "child-only".into(),
            )]),
            ..TestEnvConfig::default()
        },
        BTreeMap::new(),
    )
    .unwrap();
    let previous = std::env::var("FORKLAUNCH_ENV_PARITY_CHILD");
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--echo-child")
        .envs(environment)
        .output()
        .unwrap();
    assert!(child.status.success());
    assert_eq!(child.stdout, b"child-only\n");
    assert_eq!(std::env::var("FORKLAUNCH_ENV_PARITY_CHILD"), previous);
    for line in io::stdin().lock().lines() {
        let case: Case = serde_json::from_str(&line.unwrap()).unwrap();
        let config = TestEnvConfig {
            database_type: case.database_type.map(|value| value.parse().unwrap()),
            database: case.database.map(Into::into),
            redis: case.redis.map(Into::into),
            kafka: case.kafka.map(Into::into),
            s3: case.s3.map(Into::into),
            hmac_secret: case.hmac_secret,
            custom_vars: case.custom_vars,
        };
        let mut service = TestService::new("fixture");
        service.env = case.baseline;
        let output = match service.with_test_environment(&config) {
            Ok(service) => serde_json::json!({"environment": service.env, "tokens": {
                "AUTH":TEST_TOKENS.auth, "HMAC":TEST_TOKENS.hmac, "HMAC_INVALID":TEST_TOKENS.hmac_invalid,
            }}),
            Err(_) => serde_json::json!({"error":true}),
        };
        println!("{output}");
    }
}

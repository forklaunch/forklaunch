#![cfg(feature = "retention-calendar")]
use chrono::{DateTime, Utc};
use forklaunch_core::persistence::retention::RetentionDuration;
#[test]
fn duration_grammar_and_actual_js_overflow_order() {
    for invalid in [
        "",
        "P",
        "P0D",
        "PT1H",
        "P1W",
        "P-1D",
        "P1.5D",
        "P1D1Y",
        "P1Y1Y",
        "P4294967296D",
        "P1D ",
    ] {
        assert!(invalid.parse::<RetentionDuration>().is_err(), "{invalid}");
    }
    let fixtures = [
        (
            "2024-03-31T12:34:56.789Z",
            "P1M",
            "2024-03-02T12:34:56.789Z",
        ),
        (
            "2024-02-29T12:34:56.789Z",
            "P1Y",
            "2023-03-01T12:34:56.789Z",
        ),
        (
            "2024-02-29T12:34:56.789Z",
            "P1Y1M1D",
            "2023-01-31T12:34:56.789Z",
        ),
        (
            "2024-01-01T00:00:00.000Z",
            "P13M",
            "2022-12-01T00:00:00.000Z",
        ),
    ];
    for (now, policy, expected) in fixtures {
        let actual = policy
            .parse::<RetentionDuration>()
            .unwrap()
            .subtract_utc(now.parse::<DateTime<Utc>>().unwrap())
            .unwrap();
        assert_eq!(actual, expected.parse::<DateTime<Utc>>().unwrap());
    }
    assert!(
        "P1D"
            .parse::<RetentionDuration>()
            .unwrap()
            .subtract_utc("2024-01-01T00:00:00.000001Z".parse().unwrap())
            .is_err()
    );
}
#[test]
#[ignore = "requires Node and FORKLAUNCH_TYPESCRIPT_COMPILER; run with --include-ignored"]
fn compares_real_typescript_duration_implementation_in_utc() {
    let compiler = std::env::var("FORKLAUNCH_TYPESCRIPT_COMPILER")
        .expect("Set FORKLAUNCH_TYPESCRIPT_COMPILER for actual TS equivalence proof");
    let typescript = std::path::Path::new(&compiler)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("lib/typescript.js");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../framework/core/src/persistence/complianceTypes.ts");
    let fixtures = serde_json::json!([
        ["2024-03-31T12:34:56.789Z", "P1M"],
        ["2024-02-29T00:00:00.000Z", "P1Y1M1D"],
        ["2025-01-31T00:00:00.000Z", "P2M"],
        ["2000-02-29T00:00:00.000Z", "P100Y"],
        ["2024-01-01T00:00:00.000Z", "P0Y13M2D"]
    ]);
    let script = r"const ts=require(process.argv[1]); const fs=require('fs'); const Module=require('module');
const m=new Module('duration'); m._compile(ts.transpileModule(fs.readFileSync(process.argv[2],'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,'duration.js');
process.stdout.write(JSON.stringify(JSON.parse(process.argv[3]).map(([now,p])=>m.exports.subtractDuration(new Date(now),m.exports.parseDuration(p)).toISOString())));";
    let result = std::process::Command::new("node")
        .env("TZ", "UTC")
        .arg("-e")
        .arg(script)
        .arg(typescript)
        .arg(source)
        .arg(fixtures.to_string())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected: Vec<String> = serde_json::from_slice(&result.stdout).unwrap();
    for (case, expected) in fixtures.as_array().unwrap().iter().zip(expected) {
        let actual = case[1]
            .as_str()
            .unwrap()
            .parse::<RetentionDuration>()
            .unwrap()
            .subtract_utc(case[0].as_str().unwrap().parse().unwrap())
            .unwrap();
        assert_eq!(actual, expected.parse::<DateTime<Utc>>().unwrap());
    }
}

#[allow(dead_code)]
#[derive(forklaunch_core::persistence::compliance::ComplianceMetadata)]
struct Contact {
    #[compliance(level = "pii", nullable)]
    email: Option<String>,
}
#[test]
fn declared_policy_preserves_classification_and_exclusive_cutoff() {
    use forklaunch_core::persistence::retention::{RetentionAction, RetentionPolicy};
    let now: DateTime<Utc> = "2024-03-31T00:00:00Z".parse().unwrap();
    let policy = RetentionPolicy::for_duration_utc::<Contact>(
        now,
        "P1M".parse().unwrap(),
        RetentionAction::Anonymize,
    )
    .unwrap();
    assert_eq!(policy.anonymize_fields(), &["email"]);
    assert!(!policy.eligible(policy.cutoff(), false));
    assert!(policy.eligible(policy.cutoff() - std::time::Duration::from_secs(1), false));
    assert!(!policy.eligible(policy.cutoff() - std::time::Duration::from_secs(1), true));
}

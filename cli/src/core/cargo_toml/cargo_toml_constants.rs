// Versions written into generated Cargo.toml files. `check_blueprint_deps`
// compares these with framework-rs and blueprint-rs; `--fix` rewrites them.

// forklaunch framework-rs workspace
pub(crate) const FRAMEWORK_VERSION: &str = "0.2.5";
// forklaunch blueprint-rs workspace, released against FRAMEWORK_VERSION
pub(crate) const BLUEPRINT_VERSION: &str = "0.2.5";

// Third-party crates, keyed by dependency name. Ranges follow the newest
// requirement in the framework workspaces; `=` entries must match exactly.
pub(crate) const CRATE_VERSIONS: &[(&str, &str)] = &[
    ("apalis", "=1.0.0"),
    ("apalis-core", "=1.0.0"),
    ("apalis-postgres", "=1.0.0"),
    ("axum", "0.8"),
    ("futures-util", "0.3"),
    ("jsonwebtoken", "11"),
    ("multer", "3"),
    ("reqwest", "0.13"),
    ("schemars", "1"),
    ("serde", "1"),
    ("serde_json", "1"),
    ("sqlx", "0.9"),
    ("tokio", "1"),
    ("tokio-tungstenite", "0.30"),
    ("tower", "0.5"),
    ("uuid", "1"),
    ("zeroize", "1"),
];

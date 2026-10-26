//! Session auth wiring shared by generated Rust services, kept to the auth
//! variables so other compose generators stay independent:
//!
//! - the Rust IAM module signs RS256 session JWTs (`SESSION_JWT_*`) and its
//!   one-shot `<iam>-seed` service runs `--seed` after migrations;
//! - every JWT verifier pins one shared `JWT_ISSUER`/`JWT_AUDIENCE`;
//! - custom services verify IAM sessions through `JWKS_PUBLIC_KEY_URL` once an
//!   IAM module exists, and every Rust HTTP service reads `CORS_ORIGINS`;
//! - each service's `.env.local` gets the host-side JWKS URL and issuer/audience.
//!
//! Development values (a fresh RSA key, key id, issuer, audience and seed
//! password) are written to the git-ignored root `.env` by `local_runtime`.
use base64::Engine;
use indexmap::IndexMap;
use aws_lc_rs::encoding::{AsDer, Pkcs8V1Der};
use aws_lc_rs::rsa::{KeyPair, KeySize};

use super::*;

const ISSUER: &str = "${JWT_ISSUER:?Configure the trusted IAM session issuer}";
const AUDIENCE: &str = "${JWT_AUDIENCE:?Configure the trusted IAM session audience}";
const CORS: &str = "${CORS_ORIGINS:-}";
const SEED_PASSWORD: &str = "IAM_SEED_ADMIN_PASSWORD";

fn compose_path(root: &Path, manifest: &Value) -> PathBuf {
    root.join(
        manifest
            .get("docker_compose_path")
            .and_then(Value::as_str)
            .unwrap_or("docker-compose.yaml"),
    )
}

fn projects(manifest: &Value) -> impl Iterator<Item = &toml::Table> {
    manifest
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_table)
}

fn is_rust(project: &toml::Table) -> bool {
    project
        .get("metadata")
        .and_then(|metadata| metadata.get("language"))
        .and_then(Value::as_str)
        .is_none_or(|language| language == "rust")
}

fn iam_project(manifest: &Value) -> Option<String> {
    projects(manifest)
        .find(|project| project.get("variant").and_then(Value::as_str) == Some("iam-base") && is_rust(project))
        .and_then(|project| project.get("name").and_then(Value::as_str))
        .map(str::to_owned)
}

/// Rust HTTP services owned by the application (custom services and agents),
/// as opposed to prebuilt modules, which configure their own auth.
fn custom_services(manifest: &Value) -> Vec<String> {
    projects(manifest)
        .filter(|project| project.get("type").and_then(Value::as_str) == Some("Service") && is_rust(project))
        .filter(|project| {
            project
                .get("variant")
                .and_then(Value::as_str)
                .is_none_or(|variant| variant == "agent")
        })
        .filter_map(|project| project.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

fn ensure(env: &mut IndexMap<String, String>, key: &str, value: impl Into<String>) {
    env.entry(key.to_owned()).or_insert_with(|| value.into());
}

/// The compose service of the first module of `variants`, in either language.
fn module_service(manifest: &Value, compose: &DockerCompose, variants: &[&str]) -> Option<String> {
    projects(manifest)
        .filter(|project| project.get("variant").and_then(Value::as_str).is_some_and(|variant| variants.contains(&variant)))
        .filter_map(|project| project.get("name").and_then(Value::as_str))
        .find(|name| compose.services.contains_key(*name))
        .map(str::to_owned)
}

/// `<PROJECT>_URL` for every project with an HTTP compose service.
fn service_urls(manifest: &Value, compose: &DockerCompose) -> IndexMap<String, String> {
    projects(manifest)
        .filter_map(|project| project.get("name").and_then(Value::as_str))
        .filter_map(|name| {
            let port = compose.services.get(name)?.environment.as_ref()?.get("PORT")?;
            Some((format!("{}_URL", name.to_case(Case::UpperSnake)), format!("http://{name}:{port}")))
        })
        .collect()
}

/// Variables a TypeScript project's config injector reads (`registrations.ts`).
fn typescript_reads(root: &Path, manifest: &Value, project: &str) -> Vec<String> {
    let modules = manifest.get("modules_path").and_then(Value::as_str).unwrap_or("src");
    read_to_string(root.join(modules).join(project).join("registrations.ts"))
        .ok()
        .and_then(|source| crate::core::ast::infrastructure::env::extract_env_vars_from_source(&source).ok())
        .map(|usages| usages.into_iter().map(|usage| usage.var_name).collect())
        .unwrap_or_default()
}

/// The TypeScript generator wires a service only to TypeScript modules that
/// existed when it ran, and a TypeScript-only app reads the rest from the
/// bind-mounted `.env.local`. A mixed app runs TypeScript from a built image
/// with no `.env` files, so every value its config injector requires must be
/// in compose: `<MODULE>_URL`, the shared `HMAC_SECRET_KEY`, the IAM JWKS URL
/// (from whichever language IAM is), and Stripe credentials from the root
/// `.env`. Values the application set are kept.
fn typescript_consumers(root: &Path, manifest: &Value, compose: &mut DockerCompose) {
    let typescript: Vec<String> = projects(manifest)
        .filter(|project| !is_rust(project))
        .filter_map(|project| project.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let native: Vec<String> = projects(manifest)
        .filter(|project| is_rust(project))
        .filter_map(|project| project.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let iam = module_service(manifest, compose, &["iam-base", "iam-better-auth"]);
    let billing = module_service(manifest, compose, &["billing-base", "billing-stripe"]);
    let module_env = |module: &Option<String>, key: &str| {
        module
            .as_ref()
            .and_then(|name| compose.services.get(name)?.environment.as_ref()?.get(key).cloned())
            .filter(|value| !unset(value))
    };
    let hmac = module_env(&iam, "HMAC_SECRET_KEY").or_else(|| module_env(&billing, "HMAC_SECRET_KEY"));
    let jwks = module_env(&iam, "JWKS_PUBLIC_KEY_URL");
    let urls = service_urls(manifest, compose);
    let modules = manifest.get("modules_path").and_then(Value::as_str).unwrap_or("src");
    for project in &typescript {
        let reads = typescript_reads(root, manifest, project);
        let local = env_values(&root.join(modules).join(project).join(".env.local"));
        let own_url = format!("{}_URL", project.to_case(Case::UpperSnake));
        for (name, service) in compose.services.iter_mut() {
            let owned = !native.contains(name) && (name == project || name.starts_with(&format!("{project}-")));
            let Some(env) = service.environment.as_mut().filter(|_| owned) else { continue };
            let mut wanted: Vec<String> = reads.clone();
            // Without a readable injector, wire what the generator declared;
            // it declares HMAC_SECRET_KEY only on IAM/billing consumers.
            wanted.extend(env.keys().cloned());
            if reads.is_empty() && env.contains_key("HMAC_SECRET_KEY") {
                wanted.extend(["IAM_URL".to_owned(), "BILLING_URL".to_owned()]);
            }
            for key in wanted {
                if !env.get(&key).is_none_or(|value| unset(value)) {
                    continue;
                }
                let value = match key.as_str() {
                    "HMAC_SECRET_KEY" => hmac.clone(),
                    "JWKS_PUBLIC_KEY_URL" => jwks.clone(),
                    "STRIPE_API_KEY" | "STRIPE_WEBHOOK_SECRET" => Some(format!("${{{key}:-}}")),
                    _ if urls.contains_key(&key) => urls.get(&key).filter(|_| key != own_url).cloned(),
                    _ => local.get(&key).and_then(|value| from_local(&key, value)),
                };
                if let Some(value) = value {
                    env.insert(key, value);
                }
            }
        }
    }
}

/// A TypeScript-only app reads the generator's `.env.local` defaults (PayPal,
/// queue names, ...) from its bind mount. In compose: host-only values are
/// skipped, settings are copied, and secrets are never copied: a generator
/// placeholder becomes the default of a root `.env` reference, and anything
/// else must be set there.
fn from_local(key: &str, value: &str) -> Option<String> {
    if value.is_empty() || value.contains("localhost") || value.contains("127.0.0.1") {
        return None;
    }
    let secret = ["SECRET", "PASSWORD", "TOKEN", "PRIVATE"].iter().any(|word| key.contains(word)) || key.ends_with("_KEY");
    Some(match (secret, unset(value)) {
        (false, _) => value.to_owned(),
        (true, true) => format!("${{{key}:-{value}}}"),
        (true, false) => format!("${{{key}:?Set {key} in .env}}"),
    })
}

/// Empty or a generator placeholder.
fn unset(value: &str) -> bool {
    value.is_empty() || value.starts_with("replace-with-")
}

/// Adds the auth variables and the IAM seed service to `docker-compose.yaml`.
pub(super) fn compose(root: &Path) -> Result<()> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let path = compose_path(root, &manifest);
    if !path.is_file() {
        return Ok(());
    }
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    let iam = iam_project(&manifest).filter(|name| compose.services.contains_key(name));
    if let Some(iam) = &iam {
        let service = compose.services.get_mut(iam).expect("IAM compose service");
        let env = service.environment.get_or_insert_with(IndexMap::new);
        let signing = ["SESSION_JWT_PRIVATE_KEY_PEM", "SESSION_JWT_KEY_ID", "SESSION_JWT_ISSUER", "SESSION_JWT_AUDIENCE"];
        // All four or none: an application that removed them chose opaque sessions.
        if !signing.iter().any(|key| env.contains_key(*key)) {
            ensure(env, "SESSION_JWT_PRIVATE_KEY_PEM", "${IAM_SESSION_JWT_PRIVATE_KEY_PEM:?Configure the IAM session signing key}");
            ensure(env, "SESSION_JWT_KEY_ID", "${IAM_SESSION_JWT_KEY_ID:?Configure the IAM session signing key id}");
            ensure(env, "SESSION_JWT_ISSUER", ISSUER);
            ensure(env, "SESSION_JWT_AUDIENCE", AUDIENCE);
        }
        ensure(env, "CORS_ORIGINS", CORS);
        let seed = format!("{iam}-seed");
        if !compose.services.contains_key(&seed) {
            let source = compose.services[iam].clone();
            if let Some(DockerCommand::Multiple(command)) = &source.command
                && let Some(binary) = command.first()
            {
                let mut environment = source.environment.clone().unwrap_or_default();
                environment.insert(SEED_PASSWORD.into(), format!("${{{SEED_PASSWORD}:?Set the seeded IAM admin password}}"));
                let service = DockerService {
                    image: source.image.clone(),
                    restart: Some(Restart::No),
                    environment: Some(environment),
                    networks: source.networks.clone(),
                    working_dir: source.working_dir.clone(),
                    command: Some(DockerCommand::Multiple(vec![binary.clone(), "--seed".into()])),
                    ..Default::default()
                };
                compose.services.insert(seed, service);
            }
        }
    }
    typescript_consumers(root, &manifest, &mut compose);
    let custom = custom_services(&manifest);
    for (name, service) in compose.services.iter_mut() {
        let env = service.environment.get_or_insert_with(IndexMap::new);
        if custom.contains(name) {
            if iam.is_some() {
                let prefix = name.to_case(Case::UpperSnake);
                ensure(env, "JWKS_PUBLIC_KEY_URL", format!("${{{prefix}_JWKS_PUBLIC_KEY_URL:?Configure trusted JWKS}}"));
            }
            ensure(env, "CORS_ORIGINS", CORS);
        }
        if env.contains_key("JWKS_PUBLIC_KEY_URL") {
            ensure(env, "JWT_ISSUER", ISSUER);
            ensure(env, "JWT_AUDIENCE", AUDIENCE);
        }
    }
    write(&path, yaml_to_string(&compose)?)?;
    Ok(())
}

/// A development value for an auth variable `local_runtime` must fill.
pub(super) fn local_value(name: &str, app: &str) -> Option<String> {
    if name.ends_with("SESSION_JWT_PRIVATE_KEY_PEM") {
        return Some(development_key());
    }
    if name.ends_with("SESSION_JWT_KEY_ID") {
        return Some(format!("{app}-dev-1"));
    }
    match name {
        "JWT_ISSUER" => Some(format!("{app}-iam")),
        "JWT_AUDIENCE" => Some(format!("{app}-api")),
        SEED_PASSWORD => {
            let mut bytes = [0; 18];
            getrandom::fill(&mut bytes).expect("Failed to generate random bytes");
            Some(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
        }
        _ => None,
    }
}

/// A 2048-bit RSA PKCS#8 key as PEM, base64-encoded so it fits one `.env` line.
/// Generated with aws-lc-rs (the crypto backend the CLI already links).
fn development_key() -> String {
    let key = KeyPair::generate(KeySize::Rsa2048).expect("Failed to generate an RSA key");
    let der: Pkcs8V1Der<'static> = key.as_der().expect("Failed to encode the RSA key");
    let body = base64::engine::general_purpose::STANDARD.encode(der.as_ref());
    let mut pem = String::from("-----BEGIN PRIVATE KEY-----\n");
    for line in body.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(line).expect("base64 is ASCII"));
        pem.push('\n');
    }
    pem.push_str("-----END PRIVATE KEY-----\n");
    base64::engine::general_purpose::STANDARD.encode(pem.as_bytes())
}

fn env_values(path: &Path) -> IndexMap<String, String> {
    read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

/// Host-run (`cargo run`) settings mirroring the compose auth wiring: the IAM
/// JWKS URL on its published localhost port and the shared issuer/audience.
/// Existing values are kept.
pub(super) fn local_envs(root: &Path) -> Result<()> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let path = compose_path(root, &manifest);
    if !path.is_file() {
        return Ok(());
    }
    let compose = read_or_create_rust_docker_compose(&path)?;
    let root_env = env_values(&root.join(".env"));
    let jwks = iam_project(&manifest)
        .and_then(|iam| compose.services.get(&iam))
        .and_then(|iam| iam.ports.as_deref())
        .and_then(|ports| ports.iter().find_map(|port| crate::core::docker::compose_host_port(port)))
        .map(|port| format!("http://localhost:{port}/api/auth/jwks"));
    for project in projects(&manifest).filter(|project| is_rust(project)) {
        let Some(name) = project.get("name").and_then(Value::as_str) else { continue };
        let Some(env) = compose.services.get(name).and_then(|service| service.environment.as_ref()) else { continue };
        if !env.contains_key("JWKS_PUBLIC_KEY_URL") {
            continue;
        }
        let crate_path = project
            .get("metadata")
            .and_then(|metadata| metadata.get("cratePath"))
            .and_then(Value::as_str)
            .map_or_else(|| format!("src/{name}"), str::to_owned);
        let local = root.join(crate_path).join(".env.local");
        if !local.is_file() {
            continue;
        }
        let existing = read_to_string(&local)?;
        let present = env_values(&local);
        let mut content = existing.clone();
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        let values = [
            ("JWKS_PUBLIC_KEY_URL", jwks.clone()),
            ("JWT_ISSUER", root_env.get("JWT_ISSUER").cloned()),
            ("JWT_AUDIENCE", root_env.get("JWT_AUDIENCE").cloned()),
        ];
        for (key, value) in values {
            if let Some(value) = value.filter(|_| !present.contains_key(key)) {
                content.push_str(&format!("{key}={value}\n"));
            }
        }
        if content != existing {
            write(&local, content)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_consumers_use_the_rust_modules_wiring() {
        let manifest: Value = toml::from_str(r#"
[[projects]]
name = "iam"
variant = "iam-base"
[[projects]]
name = "billing"
variant = "billing-stripe"
[[projects]]
name = "svc"
[projects.metadata]
language = "typescript"
[[projects]]
name = "jobs"
[projects.metadata]
language = "typescript"
[[projects]]
name = "svc-native"
"#).unwrap();
        let mut compose: DockerCompose = yaml_from_str(r#"
services:
  iam:
    environment: { PORT: "8000", HMAC_SECRET_KEY: "${IAM_HMAC_SECRET_KEY:?x}", JWKS_PUBLIC_KEY_URL: "${IAM_JWKS_PUBLIC_KEY_URL:?x}" }
  billing:
    environment: { PORT: "8001", HMAC_SECRET_KEY: "${BILLING_HMAC_SECRET_KEY:?x}" }
  svc:
    environment: { HMAC_SECRET_KEY: "", JWKS_PUBLIC_KEY_URL: replace-with-jwks-public-key-url }
  jobs-worker:
    environment: { HMAC_SECRET_KEY: custom, IAM_URL: "http://custom:1" }
  jobs-plain:
    environment: { REDIS_URL: "redis://redis:6379/1" }
  svc-native:
    environment: { HMAC_SECRET_KEY: "" }
"#).unwrap();
        typescript_consumers(Path::new("/nonexistent"), &manifest, &mut compose);
        let env = |name: &str| compose.services[name].environment.clone().unwrap();
        let svc = env("svc");
        assert_eq!(svc["HMAC_SECRET_KEY"], "${IAM_HMAC_SECRET_KEY:?x}");
        assert_eq!(svc["JWKS_PUBLIC_KEY_URL"], "${IAM_JWKS_PUBLIC_KEY_URL:?x}");
        assert_eq!(svc["IAM_URL"], "http://iam:8000");
        assert_eq!(svc["BILLING_URL"], "http://billing:8001");
        // Application values are kept.
        let worker = env("jobs-worker");
        assert_eq!(worker["HMAC_SECRET_KEY"], "custom");
        assert_eq!(worker["IAM_URL"], "http://custom:1");
        assert_eq!(worker["BILLING_URL"], "http://billing:8001");
        // Non-consumers and Rust projects are untouched.
        assert!(!env("jobs-plain").contains_key("IAM_URL"));
        assert_eq!(env("svc-native")["HMAC_SECRET_KEY"], "");
    }

    fn registrations(root: &Path, project: &str, keys: &[&str]) {
        let entries: String = keys
            .iter()
            .map(|key| format!("  {key}: {{ lifetime: Lifetime.Singleton, type: string, value: getEnvVar('{key}') }},\n"))
            .collect();
        let dir = root.join("src").join(project);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("registrations.ts"), format!("const env = configInjector.chain({{\n{entries}}});\n")).unwrap();
    }

    /// A mixed app with TypeScript IAM, Rust billing and TypeScript consumers:
    /// TypeScript-to-TypeScript module URLs, keys and Stripe credentials are
    /// wired from what each project's config injector reads.
    #[test]
    fn typescript_consumers_use_modules_of_either_language() {
        let root = tempfile::tempdir().unwrap();
        registrations(root.path(), "reminders", &["IAM_URL", "BILLING_URL", "HMAC_SECRET_KEY", "JWKS_PUBLIC_KEY_URL", "REDIS_URL"]);
        registrations(root.path(), "ecommerce", &[
            "STRIPE_API_KEY", "STRIPE_WEBHOOK_SECRET", "HMAC_SECRET_KEY", "PORT", "PAYPAL_BASE_URL",
            "PAYPAL_CLIENT_SECRET", "PAYPAL_WEBHOOK_ID", "ORDER_EVENT_QUEUE", "REDIS_URL", "SIGNING_KEY",
        ]);
        std::fs::write(
            root.path().join("src/ecommerce/.env.local"),
            "STRIPE_API_KEY=replace-with-stripe-api-key\nPAYPAL_BASE_URL=https://api-m.sandbox.paypal.com\n\
             PAYPAL_CLIENT_SECRET=replace-with-paypal-client-secret\nPAYPAL_WEBHOOK_ID=replace-with-paypal-webhook-id\n\
             ORDER_EVENT_QUEUE=order-events\nREDIS_URL=redis://localhost:6379/0\nSIGNING_KEY=real-secret\n",
        )
        .unwrap();
        registrations(root.path(), "iam", &["HMAC_SECRET_KEY", "JWKS_PUBLIC_KEY_URL", "PORT"]);
        let manifest: Value = toml::from_str(r#"
language = "mixed"
modules_path = "src"
[[projects]]
name = "iam"
variant = "iam-better-auth"
[projects.metadata]
language = "typescript"
[[projects]]
name = "billing"
variant = "billing-stripe"
[projects.metadata]
language = "rust"
[[projects]]
name = "ecommerce"
variant = "ecommerce-stripe"
[projects.metadata]
language = "typescript"
[[projects]]
name = "reminders"
[projects.metadata]
language = "typescript"
"#).unwrap();
        let mut compose: DockerCompose = yaml_from_str(r#"
services:
  iam:
    environment: { PORT: "8001", HMAC_SECRET_KEY: shared, JWKS_PUBLIC_KEY_URL: "http://iam:8001/api/auth/jwks" }
  billing:
    environment: { PORT: "8002", HMAC_SECRET_KEY: "${BILLING_HMAC_SECRET_KEY:?x}", STRIPE_API_KEY: "${STRIPE_API_KEY:-}" }
  ecommerce:
    environment: { PORT: "8003", HMAC_SECRET_KEY: shared }
  ecommerce-worker:
    environment: { PORT: "8003", HMAC_SECRET_KEY: shared, STRIPE_API_KEY: replace-with-stripe-api-key }
  reminders-server:
    environment: { PORT: "8004", REDIS_URL: "redis://redis:6379/1" }
  reminders-worker:
    environment: { HMAC_SECRET_KEY: "", JWKS_PUBLIC_KEY_URL: replace-with-jwks-public-key-url }
"#).unwrap();
        typescript_consumers(root.path(), &manifest, &mut compose);
        let env = |name: &str| compose.services[name].environment.clone().unwrap();
        for worker in ["reminders-server", "reminders-worker"] {
            let worker = env(worker);
            assert_eq!(worker["IAM_URL"], "http://iam:8001");
            assert_eq!(worker["BILLING_URL"], "http://billing:8002");
            assert_eq!(worker["HMAC_SECRET_KEY"], "shared");
            assert_eq!(worker["JWKS_PUBLIC_KEY_URL"], "http://iam:8001/api/auth/jwks");
        }
        assert_eq!(env("reminders-server")["REDIS_URL"], "redis://redis:6379/1");
        for service in ["ecommerce", "ecommerce-worker"] {
            let service = env(service);
            assert_eq!(service["STRIPE_API_KEY"], "${STRIPE_API_KEY:-}");
            assert_eq!(service["STRIPE_WEBHOOK_SECRET"], "${STRIPE_WEBHOOK_SECRET:-}");
            // Only what the injector reads.
            assert!(!service.contains_key("IAM_URL"));
            // `.env.local` settings are copied; secrets only as `.env` references.
            assert_eq!(service["PAYPAL_BASE_URL"], "https://api-m.sandbox.paypal.com");
            assert_eq!(service["PAYPAL_WEBHOOK_ID"], "replace-with-paypal-webhook-id");
            assert_eq!(service["ORDER_EVENT_QUEUE"], "order-events");
            assert_eq!(service["PAYPAL_CLIENT_SECRET"], "${PAYPAL_CLIENT_SECRET:-replace-with-paypal-client-secret}");
            assert_eq!(service["SIGNING_KEY"], "${SIGNING_KEY:?Set SIGNING_KEY in .env}");
            assert!(!service.contains_key("REDIS_URL"));
        }
        // A module never points at itself, and Rust modules are left alone.
        assert!(!env("iam").contains_key("IAM_URL"));
        assert!(!env("billing").contains_key("IAM_URL"));
        assert!(!env("billing").contains_key("JWKS_PUBLIC_KEY_URL"));
    }

    #[test]
    fn development_values_are_fresh_and_single_line() {
        let key = local_value("IAM_SESSION_JWT_PRIVATE_KEY_PEM", "app").unwrap();
        assert!(!key.contains('\n'));
        let pem = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(&key).unwrap()).unwrap();
        assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----\n"));
        assert!(pem.ends_with("-----END PRIVATE KEY-----\n"));
        assert!(pem.lines().all(|line| line.len() <= 64));
        // The PEM body is the PKCS#8 DER aws-lc-rs (and so jsonwebtoken) parses back.
        let body: String = pem.lines().filter(|line| !line.starts_with("-----")).collect();
        let der = base64::engine::general_purpose::STANDARD.decode(body).unwrap();
        assert!(KeyPair::from_pkcs8(&der).is_ok());
        assert_ne!(local_value("IAM_SESSION_JWT_PRIVATE_KEY_PEM", "app").unwrap(), key);
        assert_eq!(local_value("IAM_SESSION_JWT_KEY_ID", "app").as_deref(), Some("app-dev-1"));
        assert_eq!(local_value("JWT_ISSUER", "app").as_deref(), Some("app-iam"));
        assert_eq!(local_value("JWT_AUDIENCE", "app").as_deref(), Some("app-api"));
        let password = local_value(SEED_PASSWORD, "app").unwrap();
        assert_eq!(password.len(), 24);
        assert_ne!(local_value(SEED_PASSWORD, "app").unwrap(), password);
        assert_eq!(local_value("OTHER", "app"), None);
    }
}

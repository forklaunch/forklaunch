//! Compose ordinary producer HTTP/MCP and consumer processes from the same project.
use super::*;

pub(in crate::init::rust_project) fn templates(
    dir: &Path,
    name: &str,
    description: &str,
    worker: Vec<RenderedTemplate>,
    apalis: bool,
    backend: Backend,
) -> Result<Vec<RenderedTemplate>> {
    let mut output = native_layout::service_templates(dir, name, description);
    let source = worker
        .iter()
        .find(|file| file.path == dir.join("Cargo.toml"))
        .context("worker Cargo manifest")?
        .content
        .replace(
            "worker = { package = \"forklaunch-worker-interface\"",
            "forklaunch-worker-interface = { package = \"forklaunch-worker-interface\"",
        )
        .replace(
            "framework_core = { package = \"forklaunch-core\"",
            "forklaunch-core = { package = \"forklaunch-core\"",
        );
    let worker_cargo: Value = toml::from_str(&source)?;
    let cargo = output
        .iter_mut()
        .find(|file| file.path == dir.join("Cargo.toml"))
        .context("service Cargo manifest")?;
    let mut merged: Value = toml::from_str(&cargo.content)?;
    merged
        .get_mut("package")
        .and_then(Value::as_table_mut)
        .context("service package")?
        .insert("default-run".into(), Value::String(name.into()));
    for section in ["dependencies", "build-dependencies"] {
        let target = merged
            .as_table_mut()
            .unwrap()
            .entry(section)
            .or_insert_with(|| Value::Table(Map::new()))
            .as_table_mut()
            .unwrap();
        if let Some(dependencies) = worker_cargo.get(section).and_then(Value::as_table) {
            for (key, value) in dependencies {
                if let Some(existing) = target.get_mut(key) {
                    if let (Some(existing), Some(incoming)) =
                        (existing.as_table_mut(), value.as_table())
                    {
                        if let Some(features) = incoming.get("features").and_then(Value::as_array) {
                            let current = existing
                                .entry("features")
                                .or_insert_with(|| Value::Array(Vec::new()))
                                .as_array_mut()
                                .unwrap();
                            for feature in features {
                                if !current.contains(feature) {
                                    current.push(feature.clone());
                                }
                            }
                        }
                    }
                } else {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
    }
    merged
        .get_mut("bin")
        .and_then(Value::as_array_mut)
        .context("service binaries")?
        .push(Value::Table(Map::from_iter([
            ("name".into(), Value::String(format!("{name}-worker"))),
            ("path".into(), Value::String("worker.rs".into())),
        ])));
    cargo.content = toml::to_string_pretty(&merged)?;
    for file in worker {
        let relative = file.path.strip_prefix(dir)?;
        let relative_name = relative.to_string_lossy().replace('\\', "/");
        if matches!(
            relative_name.as_str(),
            "events/typescript/package.json" | "events/typescript/tsconfig.json"
        ) {
            // Install event dependencies before Cargo emits the shared wire assets.
            output.push(file);
            continue;
        }
        if matches!(
            relative_name.as_str(),
            "Cargo.toml" | "main.rs" | "build.rs" | "Makefile" | "README.md" | ".env.example"
        ) || !relative_name.ends_with(".rs")
        {
            continue;
        }
        let content = if apalis {
            file.content
                .replace("crate::worker::", "crate::WORKER_MODULE::")
                .replace("::worker::", "forklaunch_worker_interface::")
                .replace("worker::", "forklaunch_worker_interface::")
                .replace("WORKER_MODULE", "worker")
                .replace("framework_core", "forklaunch_core")
        } else {
            file.content
        };
        let content = content.replace("crate::", "crate::consumer::").replace(
            &format!("use {}::", name.to_case(Case::Snake)),
            "use crate::consumer::",
        );
        output.push(RenderedTemplate {
            path: dir.join("consumer").join(relative),
            content,
            context: None,
        });
    }
    output
        .iter_mut()
        .find(|file| file.path == dir.join("lib.rs"))
        .unwrap()
        .content
        .push_str("\n#[path = \"consumer/lib.rs\"]\npub mod consumer;\n");
    output.push(RenderedTemplate { path: dir.join("worker.rs"), content: format!("use std::error::Error;\n\n#[tokio::main]\nasync fn main() -> Result<(), Box<dyn Error>> {{ {}::consumer::bootstrapper::run().await.map_err(|error| error.to_string().into()) }}\n", name.to_case(Case::Snake)), context: None });
    // Consumer and producer share one wire source owned by the contract package.
    for file in &mut output {
        if file
            .path
            .ends_with("consumer/domain/schemas/event_contract.rs")
            || file.path.ends_with("consumer/event_contract.rs")
        {
            file.content = "pub use contracts::jobs::*;\n".into();
        }
    }
    if output
        .iter()
        .any(|file| file.path == dir.join("consumer/persistence/compliance_schema.rs"))
    {
        let schema = output
            .iter_mut()
            .find(|file| file.path == dir.join("persistence/compliance_schema.rs"))
            .unwrap();
        schema.content = schema
            .content
            .replace("pub fn declarations()", "fn service_declarations()");
        schema.content.push_str("\n#[path=\"../consumer/persistence/compliance_schema.rs\"]\nmod queue;\npub fn declarations()->Vec<ComplianceEntity> { let mut entities=service_declarations(); entities.extend(queue::declarations()); entities }\n");
    }
    producer(&mut output, dir, name, apalis, backend)?;
    // `--migrate` means the same for both binaries: the consumer's migrations
    // (queue schema or topics), so either can run the one-shot migrate job.
    let bootstrapper = output
        .iter_mut()
        .find(|file| file.path == dir.join("bootstrapper.rs"))
        .context("producer bootstrapper")?;
    anyhow::ensure!(
        bootstrapper.content.contains("pub async fn run() -> ApplicationResult {\n"),
        "Unexpected producer bootstrapper layout"
    );
    bootstrapper.content = bootstrapper.content.replacen(
        "pub async fn run() -> ApplicationResult {\n",
        "pub async fn run() -> ApplicationResult {\n    if env::args().skip(1).eq([\"--migrate\"]) {\n        return crate::consumer::bootstrapper::run().await.map_err(|error| error.to_string().into());\n    }\n",
        1,
    );
    let assets = output
        .iter_mut()
        .find(|file| file.path == dir.join("build_assets.rs"))
        .unwrap();
    let job_path = job_include(dir);
    assets.content = assets
        .content
        .replacen("build::{", "build::{self, ", 1)
        .replacen(
            "use std::error::Error;",
            "use contracts::jobs::{WorkEvent, WorkInput};\nuse std::{env, error::Error, path::PathBuf};",
            1,
        )
        .replacen("    Ok(())", &format!("    build::worker_events(env!(\"CARGO_PKG_NAME\"), &serde_json::to_value(schemars::schema_for!(WorkInput))?, &serde_json::to_value(schemars::schema_for!(WorkEvent))?, include_str!(\"{job_path}\"), \"events\")?;\n    Ok(())"), 1);
    assets.content = assets.content.replace("Ok(Vec::new())", "Ok(vec![GeneratedAssets::worker_events(PathBuf::from(env::var(\"CARGO_MANIFEST_DIR\")?).join(\"events\"))])");
    output.push(RenderedTemplate { path:dir.join("README.md"), context:None, content:format!("# {name} worker\n\nRun the producer HTTP/MCP service with `cargo run --bin {name}` and the consumer independently with `cargo run --bin {name}-worker`. Both use the same application image in Compose. The producer authorizer rejects requests until application identity and resource permissions are implemented; trusted tenant identity is passed to the encrypted queue writer. The consumer processor also fails closed until domain processing and audited decryption are supplied. Shared job wire declarations live in `domain/schemas/jobs.rs`; Cargo emits HTTP SDKs and typed encrypted event assets. Use `--consumer-only` when creating a worker with no producer API.\n") });
    append(
        &mut output,
        &dir.join("Makefile"),
        &format!(
            "\n.PHONY: run-worker migrate-worker\nrun-worker:\n\t$(CARGO) run --package $(PACKAGE) --bin {name}-worker\nmigrate-worker:\n\t$(CARGO) run --package $(PACKAGE) --bin {name}-worker -- --migrate\n"
        ),
    )?;
    shared_domain(&mut output, dir)?;
    Ok(output)
}

fn append(files: &mut [RenderedTemplate], path: &Path, text: &str) -> Result<()> {
    files
        .iter_mut()
        .find(|file| file.path == path)
        .context("generated source missing")?
        .content
        .push_str(text);
    Ok(())
}

fn producer(
    files: &mut Vec<RenderedTemplate>,
    dir: &Path,
    name: &str,
    apalis: bool,
    backend: Backend,
) -> Result<()> {
    let host = contracts_layout::directory(dir)
        .unwrap_or_else(|| contracts_layout::new_host_directory(dir));
    let data = rust_crate_template_data(name, "Worker contracts", true);
    let jobs = render_rust_template("rust/leased-worker/event_contract.rs", &data)
        + "\n/// `operation_id` identifies the accepted job on every side: it is the queue job id\n/// (the apalis task id, the leased job id, or the Kafka record key) and the\n/// `context.operationId` the consumer sees after opening the sealed event.\n#[forklaunch_core::contract::schema]\npub struct AcceptedJob { pub operation_id: String }\n";
    files.push(RenderedTemplate {
        path: job_source(dir, &host),
        content: jobs,
        context: None,
    });
    append(files, &host.join("lib.rs"), &job_module(dir, &host))?;
    let host_cargo = files
        .iter_mut()
        .find(|file| file.path == host.join("Cargo.toml"))
        .context("contract manifest")?;
    host_cargo.content.push_str("serde = { features = [\"derive\"] }\nuuid = { features = [\"serde\", \"v4\"] }\nschemars = { features = [\"uuid1\"] }\nzeroize = {}\nforklaunch-worker-interface = { path = \"../../.forklaunch/blueprint-rs/crates/interfaces/worker-interface\", features = [\"schema\"] }\n");
    let inline = host == contracts_layout::new_host_directory(dir);
    if inline {
        let host_file = files
            .iter_mut()
            .find(|file| file.path == host.join("lib.rs"))
            .unwrap();
        host_file.content =
            contracts_layout::host_source(name, &["health".into(), "enqueue".into()])
                + &job_module(dir, &host);
        append(
            files,
            &dir.join("domain/schemas/mod.rs"),
            "\npub use crate::jobs;\n",
        )?;
    } else {
        let endpoints = if host == contracts_layout::new_host_directory(dir) {
            dir.join("api/contracts")
        } else {
            host.join("endpoints")
        };
        append(files, &endpoints.join("mod.rs"), "\npub mod enqueue;\n")?;
        let endpoint_mod = files
            .iter_mut()
            .find(|file| file.path == endpoints.join("mod.rs"))
            .unwrap();
        endpoint_mod.content = endpoint_mod.content.replace("vec![health::contract()]", "vec![health::contract(), enqueue::endpoint().contract()]")
        .replace("health::binding_manifest()", "{ let mut declarations = health::binding_manifest()?; declarations.extend(enqueue::binding_manifest()?); Ok(declarations) }");
    }
    let declaration = r#"
#[forklaunch_core::contract::api]
pub mod contract {
    use forklaunch_core::contract::{HttpMethod,endpoint::{Endpoint,JsonBody}};
    use crate::domain::schemas::jobs::{AcceptedJob,WorkInput};
    #[access(Internal)]
    pub fn enqueue() -> Endpoint<JsonBody<WorkInput>,JsonBody<AcceptedJob>> {
        Endpoint::new("Enqueue Work", "Authorize and encrypt a work item", HttpMethod::Post, "/jobs", 202).expect("valid enqueue contract")
    }
}
"#;
    if !inline {
        files.push(RenderedTemplate {path:host.join("endpoints/enqueue.rs"),content:declaration.replace("pub mod contract", "mod operations").replace("crate::domain::schemas::jobs", "crate::jobs")+"\npub use operations::{binding_manifest,Bindings,Enqueue,enqueue as endpoint};\npub use crate::jobs::AcceptedJob;\n",context:None});
    }
    append(
        files,
        &dir.join("api/controllers/mod.rs"),
        "\npub mod enqueue;\n",
    )?;
    append(
        files,
        &dir.join("api/routes/mod.rs"),
        "\npub mod enqueue;\n",
    )?;
    let routes = files
        .iter_mut()
        .find(|file| file.path == dir.join("api/routes/mod.rs"))
        .unwrap();
    routes.content = routes.content.replace(
        "health::mount(ContractRouter::new(contracts::registry()))?.finish()",
        "enqueue::mount(health::mount(ContractRouter::new(contracts::registry()))?)?.finish()",
    );
    files.push(RenderedTemplate {path:dir.join("api/routes/enqueue.rs"),context:None,content:r#"
use crate::{registrations::AppState,api::controllers::enqueue::{enqueue,ProducerAuthorization}};
use forklaunch_axum::native::ContractRouter;
pub fn mount(router:ContractRouter<AppState>)->Result<ContractRouter<AppState>,String> {
    contracts::enqueue::Bindings { enqueue: contracts::enqueue::Enqueue::bind_authorized(ProducerAuthorization,enqueue) }.mount(router)
}
"#.into()});
    files.push(RenderedTemplate {path:dir.join("api/controllers/enqueue.rs"),context:None,content:PRODUCER_POLICY.into()});
    append(
        files,
        &dir.join("domain/services/mod.rs"),
        "\npub mod replay;\n",
    )?;
    files.push(RenderedTemplate {path:dir.join("domain/services/replay.rs"),context:None,content:replay_store(apalis, backend)});
    replay_wiring(files, dir, apalis, backend)?;
    append(
        files,
        &dir.join("domain/services/mod.rs"),
        "\npub mod producer;\n",
    )?;
    let (imports, submit) = if apalis {
        ("use apalis_core::backend::TaskSink;\nuse apalis_core::task::{builder::TaskBuilder,task_id::TaskId};\nuse apalis_postgres::{Config,PostgresStorage};\nuse forklaunch_worker_interface::SealedEvent;\nuse crate::consumer::registrations;", r#"
    let pool=registrations::pool(false).await.map_err(|_|EndpointError::Unavailable)?;
    let mut queue=PostgresStorage::<SealedEvent>::new(&pool)
        .with_config(Config::default().queue(env!("CARGO_PKG_NAME")));
    // The apalis task id is the operation id, so AcceptedJob correlates with the queue row.
    let task=TaskBuilder::new(event).task_id(TaskId::from_string(operation.to_string())).build();
    let result=queue.push_task(task).await.map_err(|_|EndpointError::Unavailable);
    pool.close().await;
    result?;
"#)
    } else {
        leased_submit(backend)
    };
    let queue_expression = if apalis {
        "env!(\"CARGO_PKG_NAME\").to_owned()"
    } else {
        "config::required::<String>(\"WORKER_QUEUE\").map_err(|_|EndpointError::Unavailable)?"
    };
    files.push(RenderedTemplate {path:dir.join("domain/services/producer.rs"),context:None,content:format!(r#"
{imports}
use contracts::jobs::{{AcceptedJob,WorkEvent,WorkInput}};
use forklaunch_axum::native::EndpointError;
use forklaunch_core::encryption::{{EnvelopeKey,EnvelopeProfile}};
use forklaunch_worker_interface::{{EncryptedChannel,JobContext}};
use uuid::Uuid;
use zeroize::Zeroizing;
pub async fn enqueue(tenant:Uuid,input:WorkInput)->Result<AcceptedJob,EndpointError> {{
    if tenant.is_nil() {{ return Err(EndpointError::Unauthorized); }}
    let operation=Uuid::new_v4();
    let queue_name={queue_expression};
    let key=EnvelopeKey::from_env(EnvelopeProfile::new(env!("CARGO_PKG_NAME")).map_err(|_|EndpointError::Unavailable)?).map_err(|_|EndpointError::Unavailable)?;
    let payload=Zeroizing::new(WorkEvent {{
        context:JobContext {{tenant_id:tenant,resource_id:input.resource_id,operation_id:operation,revision:input.revision}},
        queue:queue_name.clone(),input,
    }});
    let event=EncryptedChannel::new("work-v1").and_then(|channel|channel.seal(&key,tenant,&*payload)).map_err(|_|EndpointError::InvalidRequest)?;
{submit}
    Ok(AcceptedJob {{operation_id:operation.to_string()}})
}}
"#)});
    if inline {
        append(files, &dir.join("api/controllers/enqueue.rs"), declaration)?;
        let route = files
            .iter_mut()
            .find(|file| file.path == dir.join("api/routes/enqueue.rs"))
            .unwrap();
        route.content = route.content.replace(
            "contracts::enqueue::",
            "crate::api::controllers::enqueue::contract::",
        );
    }
    Ok(())
}

/// Producer admission: the ForkLaunch request HMAC from internal callers, a clock
/// window and a one-time nonce claim. Callers share `INTERNAL_HMAC_SECRET_KEY`.
const PRODUCER_POLICY: &str = r#"
use axum::{extract::State,http::request::Parts};
use contracts::jobs::{AcceptedJob,WorkInput};
use forklaunch_axum::native::{EndpointError,EndpointPolicy,PolicyCapabilities};
use forklaunch_core::{auth::request_hmac::{HmacAuthorization,SignedBody},config,contract::{RouteContract,bindings::OriginalBodySignedPolicy}};
use std::time::{Duration,SystemTime,UNIX_EPOCH};
use uuid::Uuid;
use crate::{domain::services::{producer,replay},registrations::AppState};
/// Signed requests are accepted within this window, including future clock skew.
const MAX_SKEW:Duration=Duration::from_secs(300);
/// Service-to-service admission. Internal callers sign every request with the shared
/// `INTERNAL_HMAC_SECRET_KEY` (keyId `default`) using the ForkLaunch request HMAC over
/// the method, full path and original body bytes: generated Rust clients with
/// `with_hmac_secret`, TypeScript with `universalSdk({ hmac: { secretKey } })`.
/// Unsigned requests get 401, bad or stale signatures 403, and a replayed nonce 409.
/// The tenant comes from trusted configuration (`TENANT_ID`), never from the request.
/// Narrow this policy further (per-caller keys, resource checks) as the application needs.
#[derive(Clone)]
pub struct ProducerAuthorization;
/// `authorize_body` verifies the original bytes and claims the nonce atomically.
impl OriginalBodySignedPolicy for ProducerAuthorization {}
impl EndpointPolicy<AppState> for ProducerAuthorization {
    const CAPABILITIES:PolicyCapabilities=PolicyCapabilities::SignedInternal;
    type Context=Uuid;
    async fn authorize(&self,parts:&mut Parts,_state:&AppState,_route:&RouteContract)->Result<Uuid,EndpointError> {
        if !matches!(credential(parts).map(HmacAuthorization::parse),Some(Ok(_))) { return Err(EndpointError::Unauthorized); }
        let tenant:Uuid=config::required("TENANT_ID").map_err(|_|EndpointError::Unavailable)?;
        if tenant.is_nil() { return Err(EndpointError::Unavailable); }
        Ok(tenant)
    }
    async fn authorize_body(&self,tenant:Uuid,parts:&mut Parts,bytes:&[u8],_state:&AppState,_route:&RouteContract)->Result<Uuid,EndpointError> {
        let credential=HmacAuthorization::parse(credential(parts).ok_or(EndpointError::Unauthorized)?).map_err(|_|EndpointError::Unauthorized)?;
        let secret:String=config::required("INTERNAL_HMAC_SECRET_KEY").map_err(|_|EndpointError::Unavailable)?;
        if secret.len()<32 { return Err(EndpointError::Unavailable); }
        let body=if bytes.is_empty() { SignedBody::Absent } else { SignedBody::Bytes(bytes) };
        let now=SystemTime::now().duration_since(UNIX_EPOCH).ok().and_then(|now|i64::try_from(now.as_millis()).ok()).ok_or(EndpointError::Unavailable)?;
        if credential.key_id()!="default" || !credential.verify_at(secret.as_bytes(),parts.method.as_str(),parts.uri.path(),body,now,MAX_SKEW) {
            return Err(EndpointError::Forbidden);
        }
        // Claim only after verification; the claim outlives the signature's validity.
        let expires=credential.timestamp_unix_millis().saturating_add(i64::try_from(MAX_SKEW.as_millis()).unwrap_or(i64::MAX));
        if replay::claim(credential.nonce(),expires).await? { Ok(tenant) } else { Err(EndpointError::Conflict) }
    }
}
fn credential(parts:&Parts)->Option<&str> { parts.headers.get("authorization").and_then(|value|value.to_str().ok()) }
pub async fn enqueue(_state:State<AppState>,tenant:Uuid,input:WorkInput)->Result<AcceptedJob,EndpointError> {
    producer::enqueue(tenant,input).await
}
"#;

/// The producer's shared replay store: PostgreSQL for database queues, Redis for
/// Redis queues. Kafka producers have no shared store, so the default is per process.
fn replay_store(apalis: bool, backend: Backend) -> String {
    let postgres = |pool: &str| format!(r#"//! One-time nonce claims for signed internal calls, shared by every producer replica.
//! `--migrate` creates `forklaunch_request_nonces`; the runtime role needs INSERT, UPDATE
//! and DELETE on it. Expired claims are reclaimed in place and pruned opportunistically.
use forklaunch_axum::native::EndpointError;
use sqlx::PgPool;
pub async fn migrate(pool:&PgPool)->Result<(),sqlx::Error> {{
    sqlx::query("CREATE TABLE IF NOT EXISTS forklaunch_request_nonces (nonce text PRIMARY KEY, expires_at timestamptz NOT NULL)").execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS forklaunch_request_nonces_expiry ON forklaunch_request_nonces (expires_at)").execute(pool).await?;
    Ok(())
}}
/// `Ok(true)` when this nonce was not already claimed and unexpired.
pub async fn claim(nonce:&str,expires_unix_millis:i64)->Result<bool,EndpointError> {{
{pool}
    let claimed=sqlx::query("INSERT INTO forklaunch_request_nonces (nonce, expires_at) VALUES ($1, to_timestamp($2::double precision / 1000)) ON CONFLICT (nonce) DO UPDATE SET expires_at = EXCLUDED.expires_at WHERE forklaunch_request_nonces.expires_at < now()")
        .bind(nonce).bind(expires_unix_millis as f64).execute(&pool).await;
    // A failed prune never admits a request; it only leaves expired rows behind.
    let _=sqlx::query("DELETE FROM forklaunch_request_nonces WHERE expires_at < now() - interval '1 hour'").execute(&pool).await;
    pool.close().await;
    Ok(claimed.map_err(|_|EndpointError::Unavailable)?.rows_affected()==1)
}}
"#);
    if apalis {
        return postgres("    let pool=crate::consumer::registrations::pool(false).await.map_err(|_|EndpointError::Unavailable)?;");
    }
    match backend {
        Backend::Postgres => postgres(r#"    let url=forklaunch_core::config::required::<String>("DATABASE_URL").map_err(|_|EndpointError::Unavailable)?;
    crate::consumer::events::transport_policy().map_err(|_|EndpointError::Unavailable)?.postgres(&url).map_err(|_|EndpointError::Unavailable)?;
    let pool=sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.map_err(|_|EndpointError::Unavailable)?;"#),
        Backend::Redis => r#"//! One-time nonce claims for signed internal calls in Redis (`SET NX PX`), shared by
//! every producer replica. Keys live under the worker namespace (and `REDIS_KEY_PREFIX`).
use forklaunch_axum::native::EndpointError;
use forklaunch_core::config;
use std::time::{SystemTime,UNIX_EPOCH};
/// `Ok(true)` when this nonce was not already claimed and unexpired.
pub async fn claim(nonce:&str,expires_unix_millis:i64)->Result<bool,EndpointError> {
    let url=config::required::<String>("REDIS_URL").map_err(|_|EndpointError::Unavailable)?;
    crate::consumer::events::transport_policy().map_err(|_|EndpointError::Unavailable)?.redis(&url).map_err(|_|EndpointError::Unavailable)?;
    let namespace=config::required::<String>("WORKER_NAMESPACE").map_err(|_|EndpointError::Unavailable)?;
    let prefix=config::optional::<String>("REDIS_KEY_PREFIX").map_err(|_|EndpointError::Unavailable)?.unwrap_or_default();
    let now=SystemTime::now().duration_since(UNIX_EPOCH).ok().and_then(|now|i64::try_from(now.as_millis()).ok()).ok_or(EndpointError::Unavailable)?;
    let ttl=expires_unix_millis.saturating_sub(now).max(1000);
    let client=redis::Client::open(url).map_err(|_|EndpointError::Unavailable)?;
    let mut connection=client.get_multiplexed_async_connection().await.map_err(|_|EndpointError::Unavailable)?;
    let claimed:Option<String>=redis::cmd("SET").arg(format!("{prefix}fl_nonce:{namespace}:{nonce}")).arg("1").arg("NX").arg("PX").arg(ttl)
        .query_async(&mut connection).await.map_err(|_|EndpointError::Unavailable)?;
    Ok(claimed.is_some())
}
"#.into(),
        Backend::Kafka => r#"//! One-time nonce claims for signed internal calls. Kafka producers have no shared
//! store, so this default holds claims in this process only: it rejects replays to the
//! same replica. Run one producer replica, or replace `claim` with a shared atomic
//! store (a PostgreSQL unique insert or Redis `SET NX`) before scaling out.
use forklaunch_axum::native::EndpointError;
use std::{collections::BTreeMap,sync::{Mutex,OnceLock},time::{SystemTime,UNIX_EPOCH}};
const CAPACITY:usize=100_000;
static CLAIMS:OnceLock<Mutex<BTreeMap<String,i64>>>=OnceLock::new();
/// `Ok(true)` when this nonce was not already claimed and unexpired.
pub async fn claim(nonce:&str,expires_unix_millis:i64)->Result<bool,EndpointError> {
    let now=SystemTime::now().duration_since(UNIX_EPOCH).ok().and_then(|now|i64::try_from(now.as_millis()).ok()).ok_or(EndpointError::Unavailable)?;
    let mut claims=CLAIMS.get_or_init(Default::default).lock().map_err(|_|EndpointError::Unavailable)?;
    claims.retain(|_,expires|*expires>=now);
    if claims.contains_key(nonce) { return Ok(false); }
    if claims.len()>=CAPACITY { return Err(EndpointError::Unavailable); }
    claims.insert(nonce.to_owned(),expires_unix_millis);
    Ok(true)
}
"#.into(),
    }
}

/// Create the PostgreSQL replay table on `--migrate`, and give Redis producers a client.
fn replay_wiring(files: &mut [RenderedTemplate], dir: &Path, apalis: bool, backend: Backend) -> Result<()> {
    if apalis {
        let migrations = files
            .iter_mut()
            .find(|file| {
                file.path == dir.join("consumer/migrations/mod.rs")
                    || file.path == dir.join("migrations/mod.rs")
            })
            .context("apalis migrations")?;
        migrations.content = migrations.content.replace(
            "PostgresStorage::setup(pool).await?; Ok(())",
            "PostgresStorage::setup(pool).await?; crate::domain::services::replay::migrate(pool).await?; Ok(())",
        );
        anyhow::ensure!(migrations.content.contains("replay::migrate"), "apalis migration hook");
        return Ok(());
    }
    match backend {
        Backend::Postgres => {
            let worker = files
                .iter_mut()
                .find(|file| file.path == dir.join("consumer/worker.rs"))
                .context("database worker runtime")?;
            worker.content = worker.content.replace(
                "PostgresWorkerQueue::migrate(&pool).await?;",
                "PostgresWorkerQueue::migrate(&pool).await?;\n        crate::domain::services::replay::migrate(&pool).await?;",
            );
            anyhow::ensure!(worker.content.contains("replay::migrate"), "database migration hook");
        }
        Backend::Redis => {
            let cargo = files
                .iter_mut()
                .find(|file| file.path == dir.join("Cargo.toml"))
                .context("worker manifest")?;
            let mut manifest: Value = toml::from_str(&cargo.content)?;
            manifest
                .get_mut("dependencies")
                .and_then(Value::as_table_mut)
                .context("worker dependencies")?
                .entry("redis")
                .or_insert_with(|| {
                    toml::from_str::<Value>(
                        r#"version = "1.7"
default-features = false
features = ["tokio-rustls-comp", "tls-rustls-webpki-roots"]"#,
                    )
                    .unwrap()
                });
            cargo.content = toml::to_string_pretty(&manifest)?;
        }
        Backend::Kafka => {}
    }
    Ok(())
}

fn leased_submit(backend: Backend) -> (&'static str, &'static str) {
    match backend {
        Backend::Postgres => (
            "use std::error::Error;\nuse forklaunch_core::{config,persistence::TenantContext};\nuse forklaunch_worker_database::postgres::PostgresWorkerQueue;\nuse forklaunch_worker_interface::{AsyncWorkerQueue,Job,RetryPolicy};\nuse sqlx::postgres::PgPoolOptions;\nuse crate::consumer::events;",
            r#"
    let submit = async {
        let url=config::required::<String>("DATABASE_URL")?;
        events::transport_policy()?.postgres(&url)?;
        let pool=PgPoolOptions::new().max_connections(2).connect(&url).await?;
        let queue=PostgresWorkerQueue::new(pool.clone(),TenantContext::new(tenant.to_string())?,RetryPolicy::new(3,30)).await?;
        let result=queue.enqueue(Job::new(operation.to_string(),&queue_name,serde_json::to_string(&event)?)).await;
        pool.close().await;
        result?;
        Ok::<(),Box<dyn Error>> (())
    }.await;
    submit.map_err(|_|EndpointError::Unavailable)?;
"#,
        ),
        Backend::Redis => (
            "use std::error::Error;\nuse forklaunch_core::{config,persistence::TenantContext};\nuse forklaunch_worker_interface::{AsyncWorkerQueue,Job,RetryPolicy};\nuse forklaunch_worker_redis::leased::RedisLeasedQueue;\nuse crate::consumer::events;",
            r#"
    let submit = async {
        let url=config::required::<String>("REDIS_URL")?;
        events::transport_policy()?.redis(&url)?;
        let queue=RedisLeasedQueue::connect(&url,&config::required::<String>("WORKER_NAMESPACE")?,TenantContext::new(tenant.to_string())?,RetryPolicy::new(3,30)).await?;
        queue.enqueue(Job::new(operation.to_string(),&queue_name,serde_json::to_string(&event)?)).await?;
        Ok::<(),Box<dyn Error>> (())
    }.await;
    submit.map_err(|_|EndpointError::Unavailable)?;
"#,
        ),
        Backend::Kafka => (
            "use std::error::Error;\nuse forklaunch_core::{config,persistence::TenantContext};\nuse forklaunch_worker_interface::Job;\nuse forklaunch_worker_kafka::client::{KafkaOptions,KafkaProducer};\nuse crate::consumer::events;",
            r#"
    let submit = async {
        let mut options=KafkaOptions::new(config::required::<String>("KAFKA_BROKERS")?,config::required::<String>("WORKER_NAMESPACE")?,TenantContext::new(tenant.to_string())?,&queue_name,config::required::<String>("KAFKA_PUBLISHER_ID")?);
        if let Some(properties)=config::optional::<String>("KAFKA_CLIENT_PROPERTIES")? { options.properties=serde_json::from_str(&properties)?; }
        events::transport_policy()?.kafka(&options.brokers,&options.properties)?;
        KafkaProducer::connect(options).await?.enqueue(Job::new(operation.to_string(),&queue_name,serde_json::to_string(&event)?)).await?;
        Ok::<(),Box<dyn Error>> (())
    }.await;
    submit.map_err(|_|EndpointError::Unavailable)?;
"#,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rename_restores_distinct_consumer_binary_after_generic_package_rename() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        let dir = root.path().join("src/new-jobs");
        create_dir_all(dir.join("consumer")).unwrap();
        write(dir.join("consumer/lib.rs"), "").unwrap();
        write(dir.join("Cargo.toml"),"[package]\nname=\"new-jobs\"\n[[bin]]\nname=\"new-jobs\"\npath=\"main.rs\"\n[[bin]]\nname=\"new-jobs\"\npath=\"worker.rs\"\n").unwrap();
        write(root.path().join(".forklaunch/manifest.toml"),"[[projects]]\nname=\"new-jobs\"\ntype=\"Worker\"\n[projects.metadata]\ncratePath=\"src/new-jobs\"\n").unwrap();
        rename_sources(root.path(), "old-jobs", "new-jobs").unwrap();
        let cargo: Value =
            toml::from_str(&read_to_string(dir.join("Cargo.toml")).unwrap()).unwrap();
        assert_eq!(cargo["bin"][0]["name"].as_str(), Some("new-jobs"));
        assert_eq!(cargo["bin"][1]["name"].as_str(), Some("new-jobs-worker"));
    }

    #[test]
    fn consumer_only_still_has_one_shared_wire_owner() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("src/jobs");
        let files = consumer_contracts(
            &dir,
            "jobs",
            apalis_templates(root.path(), &dir, "jobs", "Worker").unwrap(),
        )
        .unwrap();
        let schema = files
            .iter()
            .find(|file| file.path == dir.join("domain/schemas/event_contract.rs"))
            .unwrap();
        assert_eq!(schema.content, "pub use contracts::jobs::*;\n");
        let host = files
            .iter()
            .find(|file| file.path == dir.join("domain/schemas/jobs.rs"))
            .unwrap();
        assert!(host.content.contains("pub struct WorkInput"));
        assert!(host.content.contains("forklaunch_core::contract::schema"));
        assert!(!host.content.contains("seal_work"));
        assert!(!host.content.contains("schemars::JsonSchema"));
        let manifest: Value = toml::from_str(
            &files
                .iter()
                .find(|file| file.path == dir.join("Cargo.toml"))
                .unwrap()
                .content,
        )
        .unwrap();
        assert!(manifest["dependencies"].get("contracts").is_some());
        assert!(manifest["build-dependencies"].get("contracts").is_some());
        assert_eq!(manifest["bin"].as_array().unwrap().len(), 1);
        assert!(!files.iter().any(|file| file.path == dir.join("server.rs")));
    }

    #[test]
    fn apalis_default_composes_real_service_and_independent_consumer() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("src/jobs");
        let worker = apalis_templates(root.path(), &dir, "jobs", "Worker").unwrap();
        let files = templates(&dir, "jobs", "Worker", worker, true, Backend::Postgres).unwrap();
        let source = |relative: &str| {
            &files
                .iter()
                .find(|file| file.path == dir.join(relative))
                .unwrap()
                .content
        };
        let cargo: Value = toml::from_str(source("Cargo.toml")).unwrap();
        let bins = cargo["bin"].as_array().unwrap();
        assert!(bins.iter().any(|bin| bin["name"].as_str() == Some("jobs")));
        assert!(
            bins.iter()
                .any(|bin| bin["name"].as_str() == Some("jobs-worker"))
        );
        assert!(source("worker.rs").contains("consumer::bootstrapper::run"));
        assert!(source("main.rs").contains("bootstrapper::run"));
        assert!(source("api/controllers/enqueue.rs").contains("Err(EndpointError::Unauthorized)"));
        assert!(source("domain/services/producer.rs").contains("queue.push_task(task)"));
        assert!(source("consumer/worker_hooks.rs").contains("crate::consumer::worker") == false);
        assert!(source("consumer/bootstrapper.rs").contains("crate::consumer::worker::run"));
        assert!(source("domain/mod.rs").contains("contracts::jobs"));
        assert!(source("domain/services/handler.rs").contains("ProcessorError::Configuration"));
        assert!(
            !files
                .iter()
                .any(|file| file.path.starts_with(dir.join("consumer/domain")))
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == dir.join("domain/schemas/jobs.rs"))
        );
        assert!(
            !files
                .iter()
                .any(|file| file.path.ends_with("domain/types/mod.rs"))
        );
    }
}

/// Only lifecycle/configuration stays in consumer/. Business code has one domain owner.
fn shared_domain(files: &mut Vec<RenderedTemplate>, dir: &Path) -> Result<()> {
    files.retain(|file| {
        ![
            "consumer/domain/mod.rs",
            "consumer/domain/schemas/mod.rs",
            "consumer/domain/schemas/event_contract.rs",
            "consumer/domain/services/mod.rs",
            "consumer/persistence/mod.rs",
            "consumer/build_assets.rs",
        ]
        .iter()
        .any(|path| file.path == dir.join(path))
    });
    let mut handlers = Vec::new();
    let mut has_error = false;
    let mut has_migrations = false;
    for file in files.iter_mut() {
        let Ok(relative) = file.path.strip_prefix(dir) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if let Some(name) = relative.strip_prefix("consumer/domain/services/") {
            file.path = dir.join("domain/services").join(name);
            handlers.push(name.trim_end_matches(".rs").to_owned());
        } else if relative == "consumer/domain/error.rs" {
            file.path = dir.join("domain/error.rs");
            has_error = true;
        } else if relative == "consumer/domain/schemas/events.rs" {
            file.path = dir.join("domain/services/worker_events.rs");
            file.content = file
                .content
                .replace("super::event_contract", "contracts::jobs");
            handlers.push("worker_events".into());
        } else if relative == "consumer/persistence/compliance_schema.rs" {
            file.path = dir.join("persistence/worker_compliance_schema.rs");
        } else if let Some(name) = relative.strip_prefix("consumer/migrations/") {
            file.path = dir.join("migrations").join(name);
            has_migrations |= name == "mod.rs";
        }
        file.content = file.content.replace(
            "../consumer/persistence/compliance_schema.rs",
            "worker_compliance_schema.rs",
        );
        if relative == "consumer/lib.rs" {
            file.content = file
                .content
                .replace("pub mod domain;", "pub use crate::domain;")
                .replace("pub mod migrations;", "pub use crate::migrations;")
                .replace("pub mod persistence;", "pub use crate::persistence;")
                .replace(
                    "pub use domain::schemas::event_contract;",
                    "pub use contracts::jobs as event_contract;",
                )
                .replace(
                    "pub use domain::schemas::events;",
                    "pub use crate::domain::services::worker_events as events;",
                );
        }
    }
    for handler in handlers {
        append(
            files,
            &dir.join("domain/services/mod.rs"),
            &format!("\npub mod {handler};\n"),
        )?;
    }
    append(
        files,
        &dir.join("domain/mod.rs"),
        "\npub use contracts::jobs::{WorkInput,WorkEvent};\n",
    )?;
    if has_error {
        append(files, &dir.join("domain/mod.rs"), "\npub mod error;\n")?;
    }
    if has_migrations {
        append(files, &dir.join("lib.rs"), "\npub mod migrations;\n")?;
    }
    Ok(())
}

/// Consumer-only profiles keep the same independently compiled wire owner.
pub(in crate::init::rust_project) fn consumer_contracts(
    dir: &Path,
    name: &str,
    mut files: Vec<RenderedTemplate>,
) -> Result<Vec<RenderedTemplate>> {
    let Some(index) = files
        .iter()
        .position(|file| file.path == dir.join("domain/schemas/event_contract.rs"))
    else {
        return Ok(files);
    };
    let source = files[index].content.clone();
    files[index].content = "pub use contracts::jobs::*;\n".into();
    let host = contracts_layout::directory(dir)
        .unwrap_or_else(|| contracts_layout::new_host_directory(dir));
    let cargo = files
        .iter_mut()
        .find(|file| file.path == dir.join("Cargo.toml"))
        .context("worker manifest")?;
    let mut manifest: Value = toml::from_str(&cargo.content)?;
    let dependency = Value::Table(Map::from_iter([
        ("package".into(), Value::String(format!("{name}-contracts"))),
        (
            "path".into(),
            Value::String(
                crate::core::relative_path::get_relative_path(dir, &host)
                    .to_string_lossy()
                    .into_owned(),
            ),
        ),
    ]));
    for section in ["dependencies", "build-dependencies"] {
        manifest
            .as_table_mut()
            .unwrap()
            .entry(section)
            .or_insert_with(|| Value::Table(Map::new()))
            .as_table_mut()
            .unwrap()
            .insert("contracts".into(), dependency.clone());
    }
    cargo.content = toml::to_string_pretty(&manifest)?;
    for file in &mut files {
        if matches!(
            file.path.file_name().and_then(|name| name.to_str()),
            Some("build.rs" | "build_assets.rs")
        ) {
            file.content = file.content.replace(
                "include_str!(\"domain/schemas/event_contract.rs\")",
                &format!("include_str!(\"{}\")", job_include(dir)),
            );
        }
    }
    files.push(RenderedTemplate {
        path: host.join("lib.rs"),
        content: format!(
            "//! Shared typed queue wire declarations.\n{}",
            job_module(dir, &host)
        ),
        context: None,
    });
    files.push(RenderedTemplate {
        path: job_source(dir, &host),
        content: source,
        context: None,
    });
    files.push(RenderedTemplate {path:host.join("Cargo.toml"),content:format!(r#"[package]
publish = false
name = "{name}-contracts"
version.workspace = true
edition.workspace = true
license.workspace = true
[lib]
path = "lib.rs"
[dependencies]
forklaunch-core = {{ path = "../../.forklaunch/framework-rs/crates/core", features = ["typed-schema"] }}
forklaunch-worker-interface = {{ path = "../../.forklaunch/blueprint-rs/crates/interfaces/worker-interface", features = ["schema"] }}
serde = {{ features = ["derive"] }}
schemars = {{ features = ["uuid1"] }}
uuid = {{ features = ["serde"] }}
zeroize = {{}}
"#),context:None});
    Ok(files)
}

fn job_source(dir: &Path, host: &Path) -> PathBuf {
    if host == contracts_layout::new_host_directory(dir) {
        dir.join("domain/schemas/jobs.rs")
    } else {
        host.join("jobs.rs")
    }
}
fn job_module(dir: &Path, host: &Path) -> String {
    if host == contracts_layout::new_host_directory(dir) {
        "\n#[path = \"../../domain/schemas/jobs.rs\"]\npub mod jobs;\n".into()
    } else {
        "\npub mod jobs;\n".into()
    }
}
fn job_include(dir: &Path) -> String {
    let host = contracts_layout::directory(dir)
        .unwrap_or_else(|| contracts_layout::new_host_directory(dir));
    crate::core::relative_path::get_relative_path(dir, &job_source(dir, &host))
        .to_string_lossy()
        .into_owned()
}

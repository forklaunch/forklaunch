use std::{
    collections::{HashMap, HashSet},
    fs::read_to_string,
    io::Write as IoWrite,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yml::{Value, from_str, from_value, to_string};
use termcolor::{StandardStream, WriteColor};
use uuid::Uuid;
use walkdir::WalkDir;

use super::manifest::{ManifestData, ProjectEntry};
use crate::{
    constants::{
        Database, ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE,
        ERROR_FAILED_TO_PARSE_DOCKER_COMPOSE, ERROR_FAILED_TO_READ_DOCKER_COMPOSE, Infrastructure,
        Module, Runtime, WorkerType,
    },
    core::manifest::{
        application::ApplicationManifestData, service::ServiceManifestData,
        worker::WorkerManifestData,
    },
};

const MONGO_INIT_COMMAND: &str = r#"sh -c "sleep 5; mongosh --host mongodb:27017 --eval '
rs.initiate({
    _id: \"rs0\",
    members: [
        { _id: 0, host: \"mongodb:27017\" }
    ]
});
while (!rs.isMaster().ismaster) {
    print(\"Waiting for replica set initialization...\");
    sleep(1000);
}
print(\"Replica set initialized and primary elected\");
db.getSiblingDB(\"admin\").createUser({
    user: \"mongodb\",
    pwd: \"mongodb\",
    roles: [{ role: \"root\", db: \"admin\" }]
});
'""#;

#[derive(Debug, Serialize, Default, Clone)]
pub(crate) struct DockerCompose {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
    pub(crate) volumes: IndexMap<String, DockerVolume>,
    pub(crate) networks: IndexMap<String, DockerNetwork>,
    pub(crate) services: IndexMap<String, DockerService>,

    #[serde(flatten)]
    pub(crate) additional_entries: HashMap<String, Value>,
}

impl<'de> Deserialize<'de> for DockerCompose {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};

        struct DockerComposeVisitor;

        impl<'de> Visitor<'de> for DockerComposeVisitor {
            type Value = DockerCompose;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a docker-compose object")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut compose = DockerCompose::default();

                while let Some((key, value)) = access.next_entry::<String, Value>()? {
                    match key.as_str() {
                        "version" => {
                            compose.version =
                                from_value(value).map_err(serde::de::Error::custom)?;
                        }
                        "volumes" => {
                            compose.volumes =
                                from_value(value).map_err(serde::de::Error::custom)?;
                        }
                        "networks" => {
                            compose.networks =
                                from_value(value).map_err(serde::de::Error::custom)?;
                        }
                        "services" => {
                            compose.services =
                                from_value(value).map_err(serde::de::Error::custom)?;
                        }
                        _ => {
                            compose.additional_entries.insert(key, value);
                        }
                    }
                }

                Ok(compose)
            }
        }

        deserializer.deserialize_map(DockerComposeVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub(crate) enum HealthTest {
    String(String),
    List(Vec<String>),
}

impl Default for HealthTest {
    fn default() -> Self {
        HealthTest::String(String::new())
    }
}

#[derive(Debug, Serialize, Default, Clone)]
pub(crate) struct Healthcheck {
    pub(crate) test: HealthTest,
    pub(crate) interval: String,
    pub(crate) timeout: String,
    pub(crate) retries: i32,
    pub(crate) start_period: String,

    #[serde(flatten)]
    pub(crate) additional_properties: HashMap<String, Value>,
}

impl<'de> Deserialize<'de> for Healthcheck {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};

        struct HealthcheckVisitor;

        impl<'de> Visitor<'de> for HealthcheckVisitor {
            type Value = Healthcheck;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a healthcheck object")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut healthcheck = Healthcheck::default();
                let mut additional_properties = HashMap::new();

                while let Some((key, value)) = access.next_entry::<String, Value>()? {
                    match key.as_str() {
                        "test" => {
                            healthcheck.test =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "interval" => {
                            healthcheck.interval =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "timeout" => {
                            healthcheck.timeout =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "retries" => {
                            healthcheck.retries =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "start_period" => {
                            healthcheck.start_period =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        _ => {
                            additional_properties.insert(key, value);
                        }
                    }
                }

                healthcheck.additional_properties = additional_properties;
                Ok(healthcheck)
            }
        }

        deserializer.deserialize_map(HealthcheckVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub(crate) enum Command {
    Simple(String),
    Multiple(Vec<String>),
}

impl Command {
    pub(crate) fn iter(&self) -> Box<dyn Iterator<Item = &String> + '_> {
        match self {
            Command::Simple(s) => Box::new(std::iter::once(s)),
            Command::Multiple(v) => Box::new(v.iter()),
        }
    }

    pub(crate) fn iter_mut(&mut self) -> Box<dyn Iterator<Item = &mut String> + '_> {
        match self {
            Command::Simple(s) => Box::new(std::iter::once(s)),
            Command::Multiple(v) => Box::new(v.iter_mut()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum Restart {
    #[serde(rename = "always")]
    Always,
    #[serde(rename = "unless-stopped")]
    UnlessStopped,
    #[serde(rename = "no")]
    No,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum DependencyCondition {
    #[serde(rename = "service_started")]
    ServiceStarted,
    #[serde(rename = "service_healthy")]
    ServiceHealthy,
    #[serde(rename = "service_completed_successfully")]
    ServiceCompletedSuccessfully,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct DependsOn {
    pub(crate) condition: DependencyCondition,
}

#[derive(Debug, Serialize, Default, Clone)]
pub(crate) struct DockerService {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) container_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) restart: Option<Restart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) build: Option<DockerBuild>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_environment"
    )]
    pub(crate) environment: Option<IndexMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) depends_on: Option<IndexMap<String, DependsOn>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ports: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) networks: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) volumes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) working_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) entrypoint: Option<Command>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) command: Option<Command>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) healthcheck: Option<Healthcheck>,

    #[serde(flatten)]
    pub(crate) additional_properties: HashMap<String, Value>,
}

#[allow(dead_code)]
fn deserialize_environment<'de, D>(
    deserializer: D,
) -> Result<Option<IndexMap<String, String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Option<IndexMap<String, Value>> = Option::deserialize(deserializer)?;
    let mapped = raw.map(|env| {
        env.into_iter()
            .map(|(key, value)| (key, yaml_value_to_string(value)))
            .collect::<IndexMap<_, _>>()
    });
    Ok(mapped)
}

#[allow(dead_code)]
fn yaml_value_to_string(value: Value) -> String {
    match value {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Sequence(seq) => seq
            .into_iter()
            .map(yaml_value_to_string)
            .collect::<Vec<_>>()
            .join(","),
        Value::Mapping(map) => {
            let mut pairs = Vec::new();
            for (k, v) in map {
                pairs.push(format!(
                    "{}={}",
                    yaml_value_to_string(k),
                    yaml_value_to_string(v)
                ));
            }
            pairs.join(",")
        }
        Value::Tagged(tagged) => yaml_value_to_string(tagged.value),
    }
}

fn parse_environment_value(value: Value) -> Result<Option<IndexMap<String, String>>> {
    match value {
        Value::Null => Ok(None),
        Value::Mapping(map) => {
            let mut env = IndexMap::new();
            for (k, v) in map {
                env.insert(yaml_value_to_string(k), yaml_value_to_string(v));
            }
            Ok(Some(env))
        }
        Value::Sequence(seq) => {
            let mut env = IndexMap::new();
            for item in seq {
                let entry = yaml_value_to_string(item);
                if let Some((key, val)) = entry.split_once('=') {
                    env.insert(key.to_string(), val.to_string());
                } else {
                    env.insert(entry, String::new());
                }
            }
            Ok(Some(env))
        }
        Value::String(s) => {
            if let Some((key, val)) = s.split_once('=') {
                let mut env = IndexMap::new();
                env.insert(key.to_string(), val.to_string());
                Ok(Some(env))
            } else {
                Ok(None)
            }
        }
        other => {
            let type_name = match &other {
                Value::Number(_) => "Number",
                Value::Bool(_) => "Bool",
                Value::Tagged(_) => "Tagged",
                _ => "Unknown",
            };
            Err(anyhow::anyhow!(
                "Unsupported environment value type in parse_environment_value: {} (value: {})",
                type_name,
                yaml_value_to_string(other)
            ))
        }
    }
}

impl<'de> Deserialize<'de> for DockerService {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};

        struct DockerServiceVisitor;

        impl<'de> Visitor<'de> for DockerServiceVisitor {
            type Value = DockerService;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a docker service object")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut service = DockerService::default();
                let mut additional_properties = HashMap::new();

                while let Some((key, value)) = access.next_entry::<String, Value>()? {
                    match key.as_str() {
                        "hostname" => {
                            service.hostname =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "container_name" => {
                            service.container_name =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "image" => {
                            service.image = from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "restart" => {
                            service.restart = from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "build" => {
                            service.build = from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "environment" => {
                            service.environment =
                                parse_environment_value(value).map_err(serde::de::Error::custom)?;
                        }
                        "depends_on" => {
                            service.depends_on =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "ports" => {
                            service.ports = from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "networks" => {
                            service.networks =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "volumes" => {
                            service.volumes = from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "working_dir" => {
                            service.working_dir =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "entrypoint" => {
                            service.entrypoint =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "command" => {
                            service.command = from_value(value).map_err(serde::de::Error::custom)?
                        }
                        "healthcheck" => {
                            service.healthcheck =
                                from_value(value).map_err(serde::de::Error::custom)?
                        }
                        _ => {
                            additional_properties.insert(key, value);
                        }
                    }
                }

                service.additional_properties = additional_properties;
                Ok(service)
            }
        }

        deserializer.deserialize_map(DockerServiceVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct DockerVolume {
    pub(crate) driver: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct DockerBuild {
    pub(crate) context: String,
    pub(crate) dockerfile: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct DockerNetwork {
    pub(crate) name: String,
    pub(crate) driver: String,
}

pub(crate) fn add_otel_to_docker_compose<'a>(
    app_name: &str,
    docker_compose: &'a mut DockerCompose,
    manifest_data: &ApplicationManifestData,
) -> Result<&'a mut DockerCompose> {
    let context_path = get_relative_context_path(
        &manifest_data.docker_compose_path,
        &manifest_data.modules_path,
    );

    // Ensure the network definition exists
    let network_name = format!("{}-network", app_name);
    if !docker_compose.networks.contains_key(&network_name) {
        docker_compose.networks.insert(
            network_name.clone(),
            DockerNetwork {
                name: network_name.clone(),
                driver: "bridge".to_string(),
            },
        );
    }

    // Redpanda (Kafka-compatible broker for Tempo ingest)
    docker_compose.services.insert(
        "redpanda".to_string(),
        DockerService {
            image: Some("redpandadata/redpanda:latest".to_string()),
            command: Some(Command::Simple(
                "redpanda start --overprovisioned --mode=dev-container --kafka-addr=PLAINTEXT://0.0.0.0:9092 --advertise-kafka-addr=PLAINTEXT://redpanda:9092".to_string(),
            )),
            ports: None,
            networks: Some(vec![format!("{}-network", app_name)]),
            healthcheck: Some(Healthcheck {
                test: HealthTest::List(vec![
                    "CMD".to_string(),
                    "rpk".to_string(),
                    "cluster".to_string(),
                    "health".to_string(),
                ]),
                interval: "30s".to_string(),
                timeout: "10s".to_string(),
                retries: 3,
                start_period: "30s".to_string(),
                additional_properties: HashMap::new(),
            }),
            ..Default::default()
        },
    );

    // MinIO (S3-compatible object store for Tempo, Loki, Thanos)
    if !docker_compose.services.contains_key("minio") {
        let mut minio_environment = IndexMap::new();
        minio_environment.insert("MINIO_ROOT_USER".to_string(), "minioadmin".to_string());
        minio_environment.insert("MINIO_ROOT_PASSWORD".to_string(), "minioadmin".to_string());

        docker_compose.services.insert(
            "minio".to_string(),
            DockerService {
                image: Some("minio/minio".to_string()),
                container_name: Some(format!("{}-minio", app_name)),
                restart: Some(Restart::Always),
                environment: Some(minio_environment),
                ports: Some(vec!["9000:9000".to_string(), "9001:9001".to_string()]),
                networks: Some(vec![format!("{}-network", app_name)]),
                volumes: Some(vec!["minio-data:/data".to_string()]),
                command: Some(Command::Simple(
                    "server /data --console-address :9001".to_string(),
                )),
                healthcheck: Some(Healthcheck {
                    test: HealthTest::List(vec![
                        "CMD".to_string(),
                        "mc".to_string(),
                        "ready".to_string(),
                        "local".to_string(),
                    ]),
                    interval: "30s".to_string(),
                    timeout: "10s".to_string(),
                    retries: 3,
                    start_period: "30s".to_string(),
                    additional_properties: HashMap::new(),
                }),
                ..Default::default()
            },
        );
    }

    if !docker_compose.volumes.contains_key("minio-data") {
        docker_compose.volumes.insert(
            "minio-data".to_string(),
            DockerVolume {
                driver: "local".to_string(),
            },
        );
    }

    // MinIO init (create telemetry bucket)
    docker_compose.services.insert(
        "minio-init".to_string(),
        DockerService {
            image: Some("minio/mc:latest".to_string()),
            networks: Some(vec![format!("{}-network", app_name)]),
            depends_on: Some(IndexMap::from([(
                "minio".to_string(),
                DependsOn {
                    condition: DependencyCondition::ServiceHealthy,
                },
            )])),
            entrypoint: Some(Command::Simple(
                "/bin/sh -c \"mc alias set local http://minio:9000 minioadmin minioadmin && mc mb --ignore-existing local/forklaunch-telemetry\"".to_string(),
            )),
            ..Default::default()
        },
    );

    // Tempo (distributed tracing backend)
    docker_compose.services.insert(
        "tempo".to_string(),
        DockerService {
            image: Some("grafana/tempo:latest".to_string()),
            command: Some(Command::Simple("-target=all -config.file=/etc/tempo.yaml".to_string())),
            ports: Some(vec!["3200:3200".to_string(), "4317:4317".to_string()]),
            volumes: Some(vec![format!(
                "{}/monitoring/tempo.yaml:/etc/tempo.yaml",
                context_path.to_string_lossy()
            )]),
            networks: Some(vec![format!("{}-network", app_name)]),
            depends_on: Some(IndexMap::from([
                (
                    "minio-init".to_string(),
                    DependsOn {
                        condition: DependencyCondition::ServiceCompletedSuccessfully,
                    },
                ),
                (
                    "redpanda".to_string(),
                    DependsOn {
                        condition: DependencyCondition::ServiceHealthy,
                    },
                ),
            ])),
            healthcheck: Some(Healthcheck {
                test: HealthTest::List(vec![
                    "CMD".to_string(),
                    "wget".to_string(),
                    "--no-verbose".to_string(),
                    "--tries=1".to_string(),
                    "--spider".to_string(),
                    "http://localhost:3200/ready".to_string(),
                ]),
                interval: "30s".to_string(),
                timeout: "10s".to_string(),
                retries: 3,
                start_period: "60s".to_string(),
                additional_properties: HashMap::new(),
            }),
            ..Default::default()
        },
    );

    // Loki (log aggregation)
    docker_compose.services.insert(
        "loki".to_string(),
        DockerService {
            image: Some("grafana/loki:latest".to_string()),
            command: Some(Command::Simple("-config.file=/etc/loki/local-config.yaml".to_string())),
            ports: Some(vec!["3100:3100".to_string()]),
            volumes: Some(vec![format!(
                "{}/monitoring/loki.yaml:/etc/loki/local-config.yaml",
                context_path.to_string_lossy()
            )]),
            networks: Some(vec![format!("{}-network", app_name)]),
            depends_on: Some(IndexMap::from([(
                "minio-init".to_string(),
                DependsOn {
                    condition: DependencyCondition::ServiceCompletedSuccessfully,
                },
            )])),
            healthcheck: Some(Healthcheck {
                test: HealthTest::List(vec![
                    "CMD".to_string(),
                    "wget".to_string(),
                    "--no-verbose".to_string(),
                    "--tries=1".to_string(),
                    "--spider".to_string(),
                    "http://localhost:3100/ready".to_string(),
                ]),
                interval: "30s".to_string(),
                timeout: "10s".to_string(),
                retries: 3,
                start_period: "30s".to_string(),
                additional_properties: HashMap::new(),
            }),
            ..Default::default()
        },
    );

    // Prometheus (metrics)
    if !docker_compose.volumes.contains_key("prometheus-data") {
        docker_compose.volumes.insert(
            "prometheus-data".to_string(),
            DockerVolume {
                driver: "local".to_string(),
            },
        );
    }

    docker_compose.services.insert(
        "prometheus".to_string(),
        DockerService {
            image: Some("prom/prometheus:latest".to_string()),
            ports: Some(vec!["9090:9090".to_string()]),
            volumes: Some(vec![
                format!(
                    "{}/monitoring/prometheus.yaml:/etc/prometheus/prometheus.yml",
                    context_path.to_string_lossy()
                ),
                "prometheus-data:/prometheus".to_string(),
            ]),
            command: Some(Command::Multiple(vec![
                "--config.file=/etc/prometheus/prometheus.yml".to_string(),
                "--storage.tsdb.path=/prometheus".to_string(),
                "--storage.tsdb.min-block-duration=2h".to_string(),
                "--storage.tsdb.max-block-duration=2h".to_string(),
                "--web.console.libraries=/usr/share/prometheus/console_libraries".to_string(),
                "--web.console.templates=/usr/share/prometheus/consoles".to_string(),
            ])),
            networks: Some(vec![format!("{}-network", app_name)]),
            depends_on: Some(IndexMap::from([(
                "minio-init".to_string(),
                DependsOn {
                    condition: DependencyCondition::ServiceCompletedSuccessfully,
                },
            )])),
            healthcheck: Some(Healthcheck {
                test: HealthTest::List(vec![
                    "CMD".to_string(),
                    "wget".to_string(),
                    "--no-verbose".to_string(),
                    "--tries=1".to_string(),
                    "--spider".to_string(),
                    "http://localhost:9090/-/healthy".to_string(),
                ]),
                interval: "30s".to_string(),
                timeout: "10s".to_string(),
                retries: 3,
                start_period: "30s".to_string(),
                additional_properties: HashMap::new(),
            }),
            ..Default::default()
        },
    );

    // Grafana (dashboards)
    docker_compose.services.insert(
        "grafana".to_string(),
        DockerService {
            image: Some("grafana/grafana:latest".to_string()),
            ports: Some(vec!["3000:3000".to_string()]),
            volumes: Some(vec![
                format!("{}/monitoring/grafana-provisioning/datasources:/etc/grafana/provisioning/datasources", context_path.to_string_lossy()),
                format!("{}/monitoring/grafana-provisioning/dashboards:/etc/grafana/provisioning/dashboards", context_path.to_string_lossy())
            ]),
            networks: Some(vec![format!("{}-network", app_name)]),
            healthcheck: Some(Healthcheck {
                test: HealthTest::List(vec![
                    "CMD".to_string(),
                    "wget".to_string(),
                    "--no-verbose".to_string(),
                    "--tries=1".to_string(),
                    "--spider".to_string(),
                    "http://localhost:3000/api/health".to_string(),
                ]),
                interval: "30s".to_string(),
                timeout: "10s".to_string(),
                retries: 3,
                start_period: "30s".to_string(),
                additional_properties: HashMap::new(),
            }),
            ..Default::default()
        },
    );

    // OTel Collector
    docker_compose.services.insert(
        "otel-collector".to_string(),
        DockerService {
            image: Some("otel/opentelemetry-collector:latest".to_string()),
            command: Some(Command::Simple(
                "--config=/etc/otel-collector-config.yaml".to_string(),
            )),
            ports: Some(vec!["4318:4318".to_string(), "8889:8889".to_string()]),
            volumes: Some(vec![format!(
                "{}/monitoring/otel-collector-config.yaml:/etc/otel-collector-config.yaml",
                context_path.to_string_lossy()
            )]),
            networks: Some(vec![format!("{}-network", app_name)]),
            ..Default::default()
        },
    );

    // Thanos sidecar (long-term metric storage via S3)
    docker_compose.services.insert(
        "thanos-sidecar".to_string(),
        DockerService {
            image: Some("thanosio/thanos:v0.41.0".to_string()),
            command: Some(Command::Multiple(vec![
                "sidecar".to_string(),
                "--tsdb.path=/prometheus".to_string(),
                "--prometheus.url=http://prometheus:9090".to_string(),
                "--objstore.config-file=/etc/thanos/objstore.yaml".to_string(),
            ])),
            volumes: Some(vec![
                "prometheus-data:/prometheus".to_string(),
                format!(
                    "{}/monitoring/thanos-objstore.yaml:/etc/thanos/objstore.yaml",
                    context_path.to_string_lossy()
                ),
            ]),
            networks: Some(vec![format!("{}-network", app_name)]),
            depends_on: Some(IndexMap::from([
                (
                    "prometheus".to_string(),
                    DependsOn {
                        condition: DependencyCondition::ServiceHealthy,
                    },
                ),
                (
                    "minio".to_string(),
                    DependsOn {
                        condition: DependencyCondition::ServiceHealthy,
                    },
                ),
            ])),
            ..Default::default()
        },
    );

    Ok(docker_compose)
}

pub(crate) fn add_redis_to_docker_compose<'a>(
    app_name: &str,
    docker_compose: &'a mut DockerCompose,
    environment: &mut IndexMap<String, String>,
    redis_partition: u32,
) -> Result<&'a mut DockerCompose> {
    // Ensure the network definition exists
    let network_name = format!("{}-network", app_name);
    if !docker_compose.networks.contains_key(&network_name) {
        docker_compose.networks.insert(
            network_name.clone(),
            DockerNetwork {
                name: network_name.clone(),
                driver: "bridge".to_string(),
            },
        );
    }

    environment.insert(
        "REDIS_URL".to_string(),
        format!("redis://redis:6379/{}", redis_partition),
    );
    if !docker_compose.services.contains_key("redis") {
        docker_compose.services.insert(
            "redis".to_string(),
            DockerService {
                image: Some("redis/redis-stack-server:latest".to_string()),
                container_name: Some(format!("{}-redis", app_name)),
                restart: Some(Restart::Always),
                ports: Some(vec!["6379:6379".to_string()]),
                networks: Some(vec![format!("{}-network", app_name)]),
                healthcheck: Some(Healthcheck {
                    test: HealthTest::List(vec![
                        "CMD".to_string(),
                        "redis-cli".to_string(),
                        "ping".to_string(),
                    ]),
                    interval: "10s".to_string(),
                    timeout: "5s".to_string(),
                    retries: 5,
                    start_period: "10s".to_string(),
                    additional_properties: HashMap::new(),
                }),
                ..Default::default()
            },
        );
    }
    Ok(docker_compose)
}

pub(crate) fn remove_redis_from_docker_compose<'a>(
    docker_compose: &'a mut DockerCompose,
    environment: &mut IndexMap<String, String>,
) -> Result<&'a mut DockerCompose> {
    environment.shift_remove("REDIS_URL");
    if docker_compose.services.contains_key("redis") {
        docker_compose.services.shift_remove("redis");
    }
    Ok(docker_compose)
}

pub(crate) fn add_s3_to_docker_compose<'a>(
    app_name: &str,
    service_name: &str,
    docker_compose: &'a mut DockerCompose,
    environment: &mut IndexMap<String, String>,
) -> Result<&'a mut DockerCompose> {
    // Ensure the network definition exists
    let network_name = format!("{}-network", app_name);
    if !docker_compose.networks.contains_key(&network_name) {
        docker_compose.networks.insert(
            network_name.clone(),
            DockerNetwork {
                name: network_name.clone(),
                driver: "bridge".to_string(),
            },
        );
    }

    environment.insert("S3_URL".to_string(), "http://minio:9000".to_string());
    environment.insert(
        "S3_BUCKET".to_string(),
        format!("{}-{}-dev", app_name, service_name).to_string(),
    );
    environment.insert("S3_REGION".to_string(), "us-east-1".to_string());
    environment.insert("S3_ACCESS_KEY_ID".to_string(), "minioadmin".to_string());
    environment.insert("S3_SECRET_ACCESS_KEY".to_string(), "minioadmin".to_string());
    if !docker_compose.services.contains_key("minio") {
        let mut minio_environment = IndexMap::new();
        minio_environment.insert("MINIO_ROOT_USER".to_string(), "minioadmin".to_string());
        minio_environment.insert("MINIO_ROOT_PASSWORD".to_string(), "minioadmin".to_string());

        docker_compose.services.insert(
            "minio".to_string(),
            DockerService {
                image: Some("minio/minio".to_string()),
                container_name: Some(format!("{}-minio", app_name)),
                restart: Some(Restart::Always),
                environment: Some(minio_environment),
                ports: Some(vec!["9000:9000".to_string(), "9001:9001".to_string()]),
                networks: Some(vec![format!("{}-network", app_name)]),
                volumes: Some(vec!["minio-data:/data".to_string()]),
                command: Some(Command::Simple(
                    "server /data --console-address :9001".to_string(),
                )),
                healthcheck: Some(Healthcheck {
                    test: HealthTest::List(vec![
                        "CMD".to_string(),
                        "mc".to_string(),
                        "ready".to_string(),
                        "local".to_string(),
                    ]),
                    interval: "30s".to_string(),
                    timeout: "10s".to_string(),
                    retries: 3,
                    start_period: "30s".to_string(),
                    additional_properties: HashMap::new(),
                }),
                ..Default::default()
            },
        );
    }

    if !docker_compose.volumes.contains_key("minio-data") {
        docker_compose.volumes.insert(
            "minio-data".to_string(),
            DockerVolume {
                driver: "local".to_string(),
            },
        );
    }

    Ok(docker_compose)
}

pub(crate) fn remove_s3_from_docker_compose<'a>(
    docker_compose: &'a mut DockerCompose,
    environment: &mut IndexMap<String, String>,
) -> Result<&'a mut DockerCompose> {
    environment.shift_remove("S3_URL");
    environment.shift_remove("S3_REGION");
    environment.shift_remove("S3_ACCESS_KEY_ID");
    environment.shift_remove("S3_SECRET_ACCESS_KEY");
    environment.shift_remove("S3_BUCKET");
    if docker_compose.services.contains_key("minio") {
        docker_compose.services.shift_remove("minio");
    }
    Ok(docker_compose)
}

pub(crate) fn remove_service_from_docker_compose<'a>(
    docker_compose: &'a mut DockerCompose,
    service_name: &str,
) -> Result<&'a mut DockerCompose> {
    if docker_compose.services.contains_key(service_name) {
        docker_compose.services.shift_remove(service_name);
    }
    Ok(docker_compose)
}

pub(crate) fn remove_worker_from_docker_compose<'a>(
    docker_compose: &'a mut DockerCompose,
    worker_name: &str,
) -> Result<&'a mut DockerCompose> {
    if docker_compose
        .services
        .contains_key(format!("{}-{}", worker_name, "server").as_str())
    {
        docker_compose
            .services
            .shift_remove(format!("{}-{}", worker_name, "server").as_str());
    }
    if docker_compose
        .services
        .contains_key(format!("{}-{}", worker_name, "worker").as_str())
    {
        docker_compose
            .services
            .shift_remove(format!("{}-{}", worker_name, "worker").as_str());
    }
    Ok(docker_compose)
}

pub(crate) fn add_kafka_to_docker_compose<'a>(
    app_name: &str,
    project_name: &str,
    docker_compose: &'a mut DockerCompose,
    environment: &mut IndexMap<String, String>,
) -> Result<&'a mut DockerCompose> {
    // Ensure the network definition exists
    let network_name = format!("{}-network", app_name);
    if !docker_compose.networks.contains_key(&network_name) {
        docker_compose.networks.insert(
            network_name.clone(),
            DockerNetwork {
                name: network_name.clone(),
                driver: "bridge".to_string(),
            },
        );
    }

    environment.insert("KAFKA_BROKERS".to_string(), "kafka:29092".to_string());
    environment.insert(
        "KAFKA_CLIENT_ID".to_string(),
        format!("{}-kafka-{}-client", app_name, project_name),
    );
    environment.insert(
        "KAFKA_GROUP_ID".to_string(),
        format!("{}-kafka-group", app_name),
    );

    // Generate a deterministic cluster ID based on app name using UUID v5
    let cluster_id = Uuid::new_v5(
        &Uuid::NAMESPACE_DNS,
        format!("{}-kafka-cluster", app_name).as_bytes(),
    )
    .to_string();

    // KRaft mode (Kafka without Zookeeper)
    docker_compose.services.insert(
        "kafka".to_string(),
        DockerService {
            image: Some("confluentinc/cp-kafka:latest".to_string()),
            hostname: Some("kafka".to_string()),
            container_name: Some(format!("{}-kafka", app_name)),
            ports: Some(vec!["9092:9092".to_string(), "29092:29092".to_string(), "9093:9093".to_string()]),
            environment: Some(IndexMap::from([
                ("KAFKA_BROKER_ID".to_string(), "1".to_string()),
                ("KAFKA_NODE_ID".to_string(), "1".to_string()),
                (
                    "KAFKA_LISTENER_SECURITY_PROTOCOL_MAP".to_string(),
                    "PLAINTEXT:PLAINTEXT,PLAINTEXT_HOST:PLAINTEXT,CONTROLLER:PLAINTEXT".to_string(),
                ),
                (
                    "KAFKA_LISTENERS".to_string(),
                    "PLAINTEXT://0.0.0.0:29092,CONTROLLER://0.0.0.0:9093,PLAINTEXT_HOST://0.0.0.0:9092".to_string(),
                ),
                (
                    "KAFKA_ADVERTISED_LISTENERS".to_string(),
                    "PLAINTEXT://kafka:29092,PLAINTEXT_HOST://localhost:9092".to_string(),
                ),
                (
                    "KAFKA_INTER_BROKER_LISTENER_NAME".to_string(),
                    "PLAINTEXT".to_string(),
                ),
                (
                    "KAFKA_CONTROLLER_LISTENER_NAMES".to_string(),
                    "CONTROLLER".to_string(),
                ),
                (
                    "KAFKA_PROCESS_ROLES".to_string(),
                    "broker,controller".to_string(),
                ),
                (
                    "KAFKA_CONTROLLER_QUORUM_VOTERS".to_string(),
                    "1@localhost:9093".to_string(),
                ),
                ("KAFKA_CLUSTER_ID".to_string(), cluster_id),
                (
                    "KAFKA_LOG_DIRS".to_string(),
                    "/tmp/kraft-combined-logs".to_string(),
                ),
                (
                    "KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR".to_string(),
                    "1".to_string(),
                ),
                (
                    "KAFKA_TRANSACTION_STATE_LOG_MIN_ISR".to_string(),
                    "1".to_string(),
                ),
                (
                    "KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR".to_string(),
                    "1".to_string(),
                ),
                ("KAFKA_NUM_PARTITIONS".to_string(), "1".to_string()),
                (
                    "KAFKA_GROUP_INITIAL_REBALANCE_DELAY_MS".to_string(),
                    "0".to_string(),
                ),
                (
                    "KAFKA_AUTO_CREATE_TOPICS_ENABLE".to_string(),
                    "true".to_string(),
                ),
            ])),
            networks: Some(vec![format!("{}-network", app_name)]),
            healthcheck: Some(Healthcheck {
                test: HealthTest::List(vec![
                    "CMD-SHELL".to_string(),
                    "kafka-topics --bootstrap-server kafka:29092 --list >/dev/null 2>&1 || exit 1"
                        .to_string(),
                ]),
                interval: "15s".to_string(),
                timeout: "12s".to_string(),
                retries: 5,
                start_period: "30s".to_string(),
                additional_properties: HashMap::new(),
            }),
            ..Default::default()
        },
    );

    environment.insert(
        "QUEUE_NAME".to_string(),
        format!("{}-{}-dev", app_name, project_name),
    );

    docker_compose.services.insert(
        "kafka-init".to_string(),
        DockerService {
            image: Some("confluentinc/cp-kafka:latest".to_string()),
            depends_on: Some(IndexMap::from([(
                "kafka".to_string(),
                DependsOn {
                    condition: DependencyCondition::ServiceHealthy,
                },
            )])),
            networks: Some(vec![format!("{}-network", app_name)]),
            entrypoint: Some(Command::Multiple(vec!["/bin/bash".to_string(), "-lc".to_string()])),
            command: Some(Command::Simple(format!(
                r#"set -e
until kafka-topics --bootstrap-server kafka:29092 --list; do
  echo 'Waiting for Kafka to be ready...'
  sleep 2
done
kafka-topics --bootstrap-server kafka:29092 --create --if-not-exists --topic {}-{}-dev --partitions 1 --replication-factor 1 || true
echo 'Ensure topic {}-{}-dev is fully initialized (leader assigned)'
for i in {{1..30}}; do
  DESC=$(kafka-topics --bootstrap-server kafka:29092 --describe --topic {}-{}-dev || true)
  echo "$DESC"
  if echo "$DESC" | grep -q "Leader:"; then
    if ! echo "$DESC" | grep -q "Leader: -1"; then
      echo 'Topic leader assigned.'
      break
    fi
  fi
  echo 'Waiting for topic leader assignment...'
  sleep 2
done
echo 'Topic {}-{}-dev ready'"#,
                app_name, project_name, app_name, project_name, app_name, project_name, app_name, project_name
            ))),
            ..Default::default()
        },
    );

    Ok(docker_compose)
}

pub(crate) fn add_database_to_docker_compose(
    manifest_data: &ManifestData,
    docker_compose: &mut DockerCompose,
    environment: &mut IndexMap<String, String>,
) -> Result<()> {
    if let ManifestData::Worker(worker_data) = manifest_data {
        if !worker_data.is_database_enabled {
            return Ok(());
        }
    }

    let app_name = match manifest_data {
        ManifestData::Service(service_data) => &service_data.app_name,
        ManifestData::Worker(worker_data) => &worker_data.app_name,
        _ => unreachable!(),
    };

    // Ensure the network definition exists
    let network_name = format!("{}-network", app_name);
    if !docker_compose.networks.contains_key(&network_name) {
        docker_compose.networks.insert(
            network_name.clone(),
            DockerNetwork {
                name: network_name.clone(),
                driver: "bridge".to_string(),
            },
        );
    }

    let database = match manifest_data {
        ManifestData::Service(service_data) => &service_data.database,
        ManifestData::Worker(worker_data) => &worker_data.database.as_ref().unwrap(),
        _ => unreachable!(),
    };

    let name = match manifest_data {
        ManifestData::Service(service_data) => &service_data.service_name,
        ManifestData::Worker(worker_data) => &worker_data.worker_name,
        _ => unreachable!(),
    };

    let mut active_databases = HashSet::new();
    for (_, service) in docker_compose.services.iter_mut() {
        if let Some(container_name) = &service.container_name {
            if Database::VARIANTS.contains(&container_name.as_str()) {
                active_databases.insert(container_name.as_str());
            }
        }
    }

    if !active_databases.contains(&database.as_str()) {
        match database.parse()? {
            Database::PostgreSQL => {
                docker_compose.services.insert(
                    "postgresql".to_string(),
                    DockerService {
                        image: Some("postgres:latest".to_string()),
                        container_name: Some(format!("{}-postgresql", app_name)),
                        hostname: Some("postgresql".to_string()),
                        restart: Some(Restart::UnlessStopped),
                        ports: Some(vec!["5432:5432".to_string()]),
                        environment: Some(IndexMap::from([
                            ("POSTGRES_USER".to_string(), "postgresql".to_string()),
                            ("POSTGRES_PASSWORD".to_string(), "postgresql".to_string()),
                            ("POSTGRES_HOST_AUTH_METHOD".to_string(), "trust".to_string()),
                        ])),
                        networks: Some(vec![format!("{}-network", app_name)]),
                        volumes: Some(vec![format!(
                            "{}-postgresql-data:/var/lib/postgresql",
                            app_name
                        )]),
                        healthcheck: Some(Healthcheck {
                            test: HealthTest::List(vec![
                                "CMD-SHELL".to_string(),
                                "pg_isready -U postgresql -h localhost".to_string(),
                            ]),
                            interval: "10s".to_string(),
                            timeout: "5s".to_string(),
                            retries: 5,
                            start_period: "30s".to_string(),
                            additional_properties: HashMap::new(),
                        }),
                        ..Default::default()
                    },
                );
                environment.insert("DB_NAME".to_string(), format!("{}-{}-dev", app_name, name));
                environment.insert("DB_HOST".to_string(), "postgresql".to_string());
                environment.insert("DB_USER".to_string(), "postgresql".to_string());
                environment.insert("DB_PASSWORD".to_string(), "postgresql".to_string());
                environment.insert("DB_PORT".to_string(), "5432".to_string());
                docker_compose.volumes.insert(
                    format!("{}-postgresql-data", app_name),
                    DockerVolume {
                        driver: "local".to_string(),
                    },
                );
            }
            Database::MongoDB => {
                docker_compose.services.insert(
                    "mongodb".to_string(),
                    DockerService {
                        image: Some("mongo:latest".to_string()),
                        hostname: Some("mongodb".to_string()),
                        container_name: Some(format!("{}-mongodb", app_name)),
                        restart: Some(Restart::UnlessStopped),
                        command: Some(Command::Multiple(vec![
                            "--replSet".to_string(),
                            "rs0".to_string(),
                            "--logpath".to_string(),
                            "/var/log/mongodb/mongod.log".to_string(),
                        ])),
                        environment: Some(IndexMap::from([(
                            "MONGO_INITDB_DATABASE".to_string(),
                            format!("{}-dev", app_name),
                        )])),
                        ports: Some(vec!["27017:27017".to_string()]),
                        networks: Some(vec![format!("{}-network", app_name)]),
                        volumes: Some(vec![format!("{}-mongodb-data:/data/db", app_name)]),
                        healthcheck: Some(Healthcheck {
                            test: HealthTest::List(vec![
                                "CMD-SHELL".to_string(),
                                "mongosh --eval 'db.runCommand(\"ping\").ok' localhost:27017/test --quiet".to_string(),
                            ]),
                            interval: "2s".to_string(),
                            timeout: "3s".to_string(),
                            retries: 5,
                            start_period: "3s".to_string(),
                            additional_properties: HashMap::new(),
                        }),
                        ..Default::default()
                    },
                );
                docker_compose.services.insert(
                    "mongo-init".to_string(),
                    DockerService {
                        image: Some("mongo:latest".to_string()),
                        depends_on: Some(IndexMap::from([(
                            "mongodb".to_string(),
                            DependsOn {
                                condition: DependencyCondition::ServiceStarted,
                            },
                        )])),
                        networks: Some(vec![format!("{}-network", app_name)]),
                        command: Some(Command::Simple(MONGO_INIT_COMMAND.to_string())),
                        ..Default::default()
                    },
                );
                environment.insert("DB_NAME".to_string(), format!("{}-{}-dev", app_name, name));
                environment.insert("DB_HOST".to_string(), "mongodb".to_string());
                environment.insert("DB_USER".to_string(), "mongodb".to_string());
                environment.insert("DB_PASSWORD".to_string(), "mongodb".to_string());
                environment.insert("DB_PORT".to_string(), "27017".to_string());
                docker_compose.volumes.insert(
                    format!("{}-mongodb-data", app_name),
                    DockerVolume {
                        driver: "local".to_string(),
                    },
                );
            }
            Database::MySQL => {
                docker_compose.services.insert(
                    "mysql".to_string(),
                    DockerService {
                        image: Some("mysql:latest".to_string()),
                        container_name: Some(format!("{}-mysql", app_name)),
                        hostname: Some("mysql".to_string()),
                        restart: Some(Restart::UnlessStopped),
                        ports: Some(vec!["3306:3306".to_string()]),
                        environment: Some(IndexMap::from([
                            ("MYSQL_ROOT_PASSWORD".to_string(), "mysql".to_string()),
                            ("MYSQL_USER".to_string(), "mysql".to_string()),
                            ("MYSQL_PASSWORD".to_string(), "mysql".to_string()),
                            ("MYSQL_DATABASE".to_string(), format!("{}-dev", app_name)),
                        ])),
                        networks: Some(vec![format!("{}-network", app_name)]),
                        volumes: Some(vec![format!("{}-mysql-data:/var/lib/mysql", app_name)]),
                        healthcheck: Some(Healthcheck {
                            test: HealthTest::List(vec![
                                "CMD-SHELL".to_string(),
                                "mysqladmin ping -h localhost -u $${MYSQL_USER} -p$${MYSQL_PASSWORD}"
                                    .to_string(),
                            ]),
                            interval: "10s".to_string(),
                            timeout: "5s".to_string(),
                            retries: 5,
                            start_period: "30s".to_string(),
                            additional_properties: HashMap::new(),
                        }),
                        ..Default::default()
                    },
                );
                environment.insert("DB_NAME".to_string(), format!("{}-{}-dev", app_name, name));
                environment.insert("DB_HOST".to_string(), "mysql".to_string());
                environment.insert("DB_USER".to_string(), "mysql".to_string());
                environment.insert("DB_PASSWORD".to_string(), "mysql".to_string());
                environment.insert("DB_PORT".to_string(), "3306".to_string());
                docker_compose.volumes.insert(
                    format!("{}-mysql-data", app_name),
                    DockerVolume {
                        driver: "local".to_string(),
                    },
                );
            }
            Database::MariaDB => {
                docker_compose.services.insert(
                    "mariadb".to_string(),
                    DockerService {
                        image: Some("mariadb:latest".to_string()),
                        container_name: Some(format!("{}-mariadb", app_name)),
                        hostname: Some("mariadb".to_string()),
                        restart: Some(Restart::UnlessStopped),
                        ports: Some(vec!["3306:3306".to_string()]),
                        environment: Some(IndexMap::from([
                            ("MARIADB_ROOT_PASSWORD".to_string(), "mariadb".to_string()),
                            ("MARIADB_USER".to_string(), "mariadb".to_string()),
                            ("MARIADB_PASSWORD".to_string(), "mariadb".to_string()),
                            ("MARIADB_DATABASE".to_string(), format!("{}-dev", app_name)),
                        ])),
                        networks: Some(vec![format!("{}-network", app_name)]),
                        volumes: Some(vec![format!("{}-mariadb-data:/var/lib/mysql", app_name)]),
                        healthcheck: Some(Healthcheck {
                            test: HealthTest::List(vec![
                                "CMD-SHELL".to_string(),
                                "mysqladmin ping -h localhost -u $${MARIADB_USER} -p$${MARIADB_PASSWORD}".to_string(),
                            ]),
                            interval: "10s".to_string(),
                            timeout: "5s".to_string(),
                            retries: 5,
                            start_period: "30s".to_string(),
                            additional_properties: HashMap::new(),
                        }),
                        ..Default::default()
                    },
                );
                environment.insert("DB_NAME".to_string(), format!("{}-{}-dev", app_name, name));
                environment.insert("DB_HOST".to_string(), "mariadb".to_string());
                environment.insert("DB_USER".to_string(), "mariadb".to_string());
                environment.insert("DB_PASSWORD".to_string(), "mariadb".to_string());
                environment.insert("DB_PORT".to_string(), "3306".to_string());
                docker_compose.volumes.insert(
                    format!("{}-mariadb-data", app_name),
                    DockerVolume {
                        driver: "local".to_string(),
                    },
                );
            }
            Database::MsSQL => {
                docker_compose.services.insert(
                    "mssql".to_string(),
                    DockerService {
                        image: Some("mcr.microsoft.com/mssql/server:latest".to_string()),
                        container_name: Some(format!("{}-mssql", app_name)),
                        hostname: Some("mssql".to_string()),
                        restart: Some(Restart::UnlessStopped),
                        ports: Some(vec!["1433:1433".to_string()]),
                        environment: Some(IndexMap::from([
                            ("ACCEPT_EULA".to_string(), "Y".to_string()),
                            ("SA_PASSWORD".to_string(), "Mssql123!".to_string()),
                            ("MSSQL_PID".to_string(), "Developer".to_string()),
                        ])),
                        networks: Some(vec![format!("{}-network", app_name)]),
                        volumes: Some(vec![format!("{}-mssql-data:/var/opt/mssql", app_name)]),
                        healthcheck: Some(Healthcheck {
                            test: HealthTest::List(vec![
                                "CMD-SHELL".to_string(),
                                "/opt/mssql-tools/bin/sqlcmd -S localhost -U sa -P \"$${SA_PASSWORD}\" -Q 'SELECT 1' || exit 1".to_string(),
                            ]),
                            interval: "10s".to_string(),
                            timeout: "5s".to_string(),
                            retries: 5,
                            start_period: "30s".to_string(),
                            additional_properties: HashMap::new(),
                        }),
                        ..Default::default()
                    },
                );
                environment.insert("DB_NAME".to_string(), format!("{}-{}-dev", app_name, name));
                environment.insert("DB_HOST".to_string(), "mssql".to_string());
                environment.insert("DB_USER".to_string(), "sa".to_string());
                environment.insert("DB_PASSWORD".to_string(), "Mssql123!".to_string());
                environment.insert("DB_PORT".to_string(), "1433".to_string());
                docker_compose.volumes.insert(
                    format!("{}-mssql-data", app_name),
                    DockerVolume {
                        driver: "local".to_string(),
                    },
                );
            }
            Database::SQLite => {
                environment.insert("DB_NAME".to_string(), format!("{}-{}-dev", app_name, name));
                environment.shift_remove("DB_HOST");
                environment.shift_remove("DB_USER");
                environment.shift_remove("DB_PASSWORD");
                environment.shift_remove("DB_PORT");
            }
            Database::BetterSQLite => {
                environment.insert("DB_NAME".to_string(), format!("{}-{}-dev", app_name, name));
                environment.shift_remove("DB_HOST");
                environment.shift_remove("DB_USER");
                environment.shift_remove("DB_PASSWORD");
                environment.shift_remove("DB_PORT");
            }
            Database::LibSQL => {
                environment.insert("DB_NAME".to_string(), format!("{}-{}-dev", app_name, name));
                environment.shift_remove("DB_HOST");
                environment.shift_remove("DB_USER");
                environment.shift_remove("DB_PASSWORD");
                environment.shift_remove("DB_PORT");
            }
        }
    }

    Ok(())
}

pub(crate) fn clean_up_unused_infrastructure_services(
    docker_compose: &mut DockerCompose,
    projects: Vec<ProjectEntry>,
) -> Result<()> {
    let mut infrastructure_in_use = HashSet::new();

    for project in projects {
        if let Some(resources) = project.resources {
            if let Some(database) = resources.database {
                infrastructure_in_use.insert(database);
            }
            if let Some(cache) = resources.cache {
                infrastructure_in_use.insert(cache);
            }
            if let Some(queue) = resources.queue {
                let queue = queue.clone();
                infrastructure_in_use.insert(queue);
            }
        }
    }

    let mut all_infrastructure_set = HashSet::from(Database::VARIANTS);
    all_infrastructure_set.extend(Infrastructure::VARIANTS);
    all_infrastructure_set.extend(
        WorkerType::VARIANTS
            .iter()
            .filter(|r#type| r#type.parse::<WorkerType>().ok() != Some(WorkerType::Database)),
    );

    let all_infrastructure_set = all_infrastructure_set
        .iter()
        .map(|infrastructure| infrastructure.to_string())
        .collect::<HashSet<String>>();

    let unused_infrastructure = all_infrastructure_set
        .difference(&infrastructure_in_use)
        .flat_map(|component| match component.parse::<Infrastructure>() {
            Ok(Infrastructure::Redis) => vec!["redis".to_string()],
            _ => match component.parse::<Database>() {
                Ok(Database::MongoDB) => vec!["mongodb".to_string(), "mongo-init".to_string()],
                _ => vec![component.to_string()],
            },
        });

    for infrastructure in unused_infrastructure {
        docker_compose
            .services
            .shift_remove(&infrastructure.to_string());
    }

    Ok(())
}

fn add_base_definition_to_docker_compose(
    app_name: &str,
    name: &str,
    docker_compose_path: &PathBuf,
    docker_compose_string: Option<String>,
) -> Result<(DockerCompose, i32, IndexMap<String, String>)> {
    let mut docker_compose: DockerCompose =
        if let Some(inner_docker_compose_string) = docker_compose_string {
            from_str(&inner_docker_compose_string)
                .with_context(|| ERROR_FAILED_TO_PARSE_DOCKER_COMPOSE)?
        } else {
            from_str(
                &read_to_string(docker_compose_path)
                    .with_context(|| ERROR_FAILED_TO_READ_DOCKER_COMPOSE)?,
            )
            .with_context(|| ERROR_FAILED_TO_PARSE_DOCKER_COMPOSE)?
        };

    let network_name = format!("{}-network", app_name);
    if !docker_compose.networks.contains_key(&network_name) {
        docker_compose.networks.insert(
            network_name.clone(),
            DockerNetwork {
                name: network_name,
                driver: "bridge".to_string(),
            },
        );
    }

    let mut port_number = 8000 - 1;
    for (_, value) in docker_compose.services.iter() {
        if let Some(ports) = &value.ports {
            for port in ports {
                if let Some(port_num) = port.split(':').next() {
                    if let Ok(num) = port_num.parse::<i32>() {
                        if num >= 8000 && num < 9000 && num > port_number && num != 8889 {
                            port_number = num;
                        }
                    }
                }
            }
        }
    }

    port_number += 1;

    let mut environment = IndexMap::new();
    environment.insert("NODE_ENV".to_string(), "development".to_string());
    environment.insert("HOST".to_string(), "0.0.0.0".to_string());
    environment.insert("PROTOCOL".to_string(), "http".to_string());
    environment.insert("PORT".to_string(), port_number.to_string());
    environment.insert("VERSION".to_string(), "v1".to_string());
    environment.insert("DOCS_PATH".to_string(), "/docs".to_string());
    environment.insert(
        "OTEL_EXPORTER_OTLP_ENDPOINT".to_string(),
        "http://otel-collector:4318".to_string(),
    );
    environment.insert(
        "OTEL_SERVICE_NAME".to_string(),
        format!("{}-{}", app_name, name),
    );

    Ok((docker_compose, port_number, environment))
}

fn create_base_service(
    app_name: &str,
    component_name: &str,
    runtime: &str,
    database: &Option<String>,
    is_cache_enabled: bool,
    is_s3_enabled: bool,
    is_in_memory_database: bool,
    port_number: Option<i32>,
    environment: IndexMap<String, String>,
    volumes: Vec<String>,
    container_name_suffix: Option<&str>,
    entrypoint_command: &str,
    additional_depends_on: Vec<String>,
    context_path: &PathBuf,
) -> DockerService {
    let mut depends_on = vec![];

    if is_cache_enabled {
        depends_on.push("redis".to_string());
    }
    if is_s3_enabled {
        depends_on.push("minio".to_string());
    }
    if !is_in_memory_database && database.is_some() {
        depends_on.push(database.clone().unwrap());
    }

    depends_on.extend(additional_depends_on);

    DockerService {
        hostname: Some(component_name.to_string()),
        container_name: Some(format!(
            "{}-{}-{}{}",
            app_name,
            component_name,
            runtime,
            container_name_suffix.map_or_else(|| "".to_string(), |suffix| format!("-{}", suffix))
        )),
        restart: Some(Restart::Always),
        build: Some(DockerBuild {
            context: context_path.to_string_lossy().to_string(),
            dockerfile: format!("./Dockerfile"),
        }),
        image: Some(format!(
            "{}-{}-{}:latest",
            app_name, component_name, runtime
        )),
        environment: Some(environment),
        depends_on: if depends_on.len() > 0 {
            Some(
                depends_on
                    .into_iter()
                    .map(|service_name| {
                        (
                            service_name,
                            DependsOn {
                                condition: DependencyCondition::ServiceStarted,
                            },
                        )
                    })
                    .collect(),
            )
        } else {
            None
        },
        ports: if let Some(port_number) = port_number {
            Some(vec![format!("{}:{}", port_number, port_number)])
        } else {
            None
        },
        networks: Some(vec![format!("{}-network", app_name)]),
        volumes: Some(volumes),
        working_dir: Some(format!("/{}/{}", app_name, component_name)),
        entrypoint: Some(Command::Multiple(vec![
            match runtime {
                "node" => "pnpm".to_string(),
                "bun" => "bun".to_string(),
                _ => unreachable!(),
            },
            "run".to_string(),
            entrypoint_command.to_string(),
        ])),
        healthcheck: if let Some(port_number) = port_number {
            Some(Healthcheck {
                test: HealthTest::List(vec![
                    "CMD".to_string(),
                    "curl".to_string(),
                    "-f".to_string(),
                    format!("http://localhost:{}/health", port_number),
                ]),
                interval: "30s".to_string(),
                timeout: "10s".to_string(),
                retries: 3,
                start_period: "40s".to_string(),
                additional_properties: HashMap::new(),
            })
        } else {
            None
        },
        ..Default::default()
    }
}

fn add_iam_environment_variables_to_docker_compose(
    _app_name: &str,
    service_name: &str,
    port_number: i32,
    projects: Vec<ProjectEntry>,
    docker_compose: &DockerCompose,
    environment: &mut IndexMap<String, String>,
    hmac_secret: &str,
) -> Result<()> {
    environment.insert("HMAC_SECRET_KEY".to_string(), hmac_secret.to_string());

    let iam_project = projects.iter().find(|project| project.name == "iam");
    let iam_project_variant = iam_project
        .unwrap()
        .variant
        .as_ref()
        .unwrap()
        .parse::<Module>()
        .unwrap();

    let iam_port = if service_name == "iam" {
        port_number
    } else {
        docker_compose
            .services
            .get("iam")
            .unwrap()
            .ports
            .as_ref()
            .unwrap()[0]
            .split(":")
            .next()
            .unwrap()
            .parse::<i32>()
            .unwrap()
    };

    environment.insert(
        "JWKS_PUBLIC_KEY_URL".to_string(),
        match iam_project_variant {
            Module::BaseIam => "replace-with-jwks-public-key-url".to_string(),
            Module::BetterAuthIam => format!("http://iam:{}/api/auth/jwks", iam_port).to_string(),
            _ => "replace-with-jwks-public-key-url".to_string(),
        },
    );

    Ok(())
}

pub(crate) fn add_service_definition_to_docker_compose(
    manifest_data: &ServiceManifestData,
    base_path: &Path,
    docker_compose_string: Option<String>,
) -> Result<String> {
    let docker_compose_path = if let Some(docker_compose_path) = &manifest_data.docker_compose_path
    {
        &base_path.join(docker_compose_path)
    } else {
        &base_path.join("docker-compose.yaml")
    };

    let context_path = get_relative_context_path(
        &manifest_data.docker_compose_path,
        &manifest_data.modules_path,
    );

    let (mut docker_compose, port_number, mut environment) = add_base_definition_to_docker_compose(
        &manifest_data.app_name,
        &manifest_data.service_name,
        docker_compose_path,
        docker_compose_string,
    )?;

    if manifest_data.is_better_auth {
        environment.insert(
            "BETTER_AUTH_SECRET".to_string(),
            manifest_data.generated_better_auth_secret.clone(),
        );
        environment.insert("BETTER_AUTH_BASE_PATH".to_string(), "/api/auth".to_string());
        environment.insert(
            "CORS_ORIGINS".to_string(),
            "http://localhost:3001".to_string(),
        );
    }
    if manifest_data.is_stripe {
        environment.insert(
            "STRIPE_API_KEY".to_string(),
            "replace-with-stripe-api-key".to_string(),
        );
        environment.insert(
            "STRIPE_WEBHOOK_SECRET".to_string(),
            "replace-with-stripe-webhook-secret".to_string(),
        );
    }

    if manifest_data.is_iam_configured {
        add_iam_environment_variables_to_docker_compose(
            &manifest_data.app_name,
            &manifest_data.service_name,
            port_number,
            manifest_data.projects.clone(),
            &docker_compose,
            &mut environment,
            &manifest_data.generated_hmac_secret,
        )?;
    }

    if manifest_data.is_cache_enabled {
        add_redis_to_docker_compose(
            &manifest_data.app_name,
            &mut docker_compose,
            &mut environment,
            0,
        )
        .with_context(|| {
            format!(
                "{} Tried to add Redis to docker compose",
                ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE
            )
        })?;
    }

    if manifest_data.is_s3_enabled {
        add_s3_to_docker_compose(
            &manifest_data.app_name,
            &manifest_data.service_name,
            &mut docker_compose,
            &mut environment,
        )
        .with_context(|| {
            format!(
                "{} Tried to add S3 to docker compose",
                ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE
            )
        })?;
    }

    add_database_to_docker_compose(
        &ManifestData::Service(manifest_data),
        &mut docker_compose,
        &mut environment,
    )
    .with_context(|| {
        format!(
            "{} Tried to add database to docker compose",
            ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE
        )
    })?;

    let volumes = vec![
        format!(
            "{}/{}:/{}/{}",
            context_path.to_string_lossy(),
            manifest_data.service_name,
            manifest_data.app_name,
            manifest_data.service_name
        ),
        format!(
            "/{}/{}/dist",
            manifest_data.app_name, manifest_data.service_name
        ),
        format!(
            "/{}/{}/node_modules",
            manifest_data.app_name, manifest_data.service_name
        ),
        format!("/{}/core/node_modules", manifest_data.app_name),
        format!("/{}/monitoring/node_modules", manifest_data.app_name),
        format!("/{}/client-sdk/node_modules", manifest_data.app_name),
        format!("/{}/node_modules", manifest_data.app_name),
    ];

    let service_name = manifest_data.service_name.clone();
    if !docker_compose.services.contains_key(&service_name) {
        docker_compose.services.insert(
            service_name,
            create_base_service(
                &manifest_data.app_name,
                &manifest_data.service_name,
                &manifest_data.runtime,
                &Some(manifest_data.database.clone()),
                manifest_data.is_cache_enabled,
                manifest_data.is_s3_enabled,
                manifest_data.is_in_memory_database,
                Some(port_number),
                environment,
                volumes,
                None,
                "dev",
                vec![],
                &context_path,
            ),
        );
    }

    Ok(to_string(&docker_compose)
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE)?)
}

fn get_relative_context_path(
    docker_compose_path: &Option<String>,
    modules_path: &String,
) -> PathBuf {
    let context_rel = if let Some(raw) = docker_compose_path {
        let compose_dir = Path::new(raw).parent().unwrap_or(Path::new(""));
        let depth = compose_dir.components().count();
        let mut up = PathBuf::new();
        for _ in 0..depth {
            up.push("..");
        }
        up
    } else {
        PathBuf::from(".")
    };
    context_rel.join(&modules_path)
}

pub(crate) fn add_worker_definition_to_docker_compose(
    manifest_data: &WorkerManifestData,
    app_root_path: &Path,
    docker_compose_string: Option<String>,
) -> Result<String> {
    let docker_compose_path = if let Some(docker_compose_path) = &manifest_data.docker_compose_path
    {
        &app_root_path.join(docker_compose_path)
    } else {
        &app_root_path.join("docker-compose.yaml")
    };

    let context_path = get_relative_context_path(
        &manifest_data.docker_compose_path,
        &manifest_data.modules_path,
    );

    let (mut docker_compose, port_number, mut environment) = add_base_definition_to_docker_compose(
        &manifest_data.app_name,
        &manifest_data.worker_name,
        docker_compose_path,
        docker_compose_string,
    )?;

    environment.insert(
        "QUEUE_NAME".to_string(),
        format!(
            "{}-{}-dev",
            &manifest_data.app_name, &manifest_data.worker_name
        ),
    );

    if manifest_data.is_cache_enabled {
        add_redis_to_docker_compose(
            &manifest_data.app_name,
            &mut docker_compose,
            &mut environment,
            manifest_data.redis_partition,
        )
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE)?;
    } else if manifest_data.is_database_enabled {
        add_database_to_docker_compose(
            &ManifestData::Worker(manifest_data),
            &mut docker_compose,
            &mut environment,
        )
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE)?;
    } else if manifest_data.is_kafka_enabled {
        add_kafka_to_docker_compose(
            &manifest_data.app_name,
            &manifest_data.worker_name,
            &mut docker_compose,
            &mut environment,
        )
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE)?;
    }

    if manifest_data.is_iam_configured {
        add_iam_environment_variables_to_docker_compose(
            &manifest_data.app_name,
            &manifest_data.worker_name,
            port_number,
            manifest_data.projects.clone(),
            &docker_compose,
            &mut environment,
            &manifest_data.generated_hmac_secret,
        )?;
    }

    let volumes = vec![
        format!(
            "{}/{}:/{}/{}",
            context_path.to_string_lossy(),
            manifest_data.worker_name,
            manifest_data.app_name,
            manifest_data.worker_name
        ),
        format!(
            "/{}/{}/dist",
            manifest_data.app_name, manifest_data.worker_name
        ),
        format!(
            "/{}/{}/node_modules",
            manifest_data.app_name, manifest_data.worker_name
        ),
        format!("/{}/core/node_modules", manifest_data.app_name),
        format!("/{}/node_modules", manifest_data.app_name),
    ];

    let server_service_name = format!("{}-server", manifest_data.worker_name);
    if !docker_compose.services.contains_key(&server_service_name) {
        docker_compose.services.insert(
            server_service_name.clone(),
            create_base_service(
                &manifest_data.app_name,
                &manifest_data.worker_name,
                &manifest_data.runtime,
                &manifest_data.database,
                manifest_data.is_cache_enabled,
                false,
                manifest_data.is_in_memory_database,
                Some(port_number),
                environment.clone(),
                volumes.clone(),
                Some("server"),
                "dev:server",
                vec![],
                &context_path,
            ),
        );
    }

    let worker_service_name = format!("{}-worker", manifest_data.worker_name);
    if !docker_compose.services.contains_key(&worker_service_name) {
        let mut worker_dependencies = vec![server_service_name.clone()];

        if manifest_data.is_kafka_enabled {
            worker_dependencies.push("kafka-init".to_string());
        }

        let mut worker_service = create_base_service(
            &manifest_data.app_name,
            &manifest_data.worker_name,
            &manifest_data.runtime,
            &manifest_data.database,
            manifest_data.is_cache_enabled,
            false,
            manifest_data.is_in_memory_database,
            None,
            environment.clone(),
            volumes.clone(),
            Some("worker"),
            "dev:worker",
            worker_dependencies,
            &context_path,
        );

        if manifest_data.is_kafka_enabled {
            if let Some(ref mut depends_on) = worker_service.depends_on {
                if let Some(kafka_init_dep) = depends_on.get_mut("kafka-init") {
                    kafka_init_dep.condition = DependencyCondition::ServiceCompletedSuccessfully;
                }
            }
        }

        docker_compose
            .services
            .insert(worker_service_name.clone(), worker_service);
    }

    Ok(to_string(&docker_compose)
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE)?)
}

fn is_valid_docker_compose(compose: &DockerCompose) -> bool {
    if compose.services.is_empty() {
        return false;
    }

    let has_docker_service_fields = compose.services.values().any(|service| {
        service.image.is_some()
            || service.build.is_some()
            || service.container_name.is_some()
            || service.ports.is_some()
            || service.environment.is_some()
            || service.volumes.is_some()
            || service.depends_on.is_some()
    });

    if !has_docker_service_fields {
        return false;
    }

    if compose.additional_entries.contains_key("lockfileVersion")
        || compose.additional_entries.contains_key("packages")
        || compose.additional_entries.contains_key("dependencies")
    {
        return false;
    }

    if compose.additional_entries.contains_key("apiVersion")
        || compose.additional_entries.contains_key("kind")
        || compose.additional_entries.contains_key("metadata")
        || compose.additional_entries.contains_key("spec")
    {
        return false;
    }

    if compose.additional_entries.contains_key("openapi")
        || compose.additional_entries.contains_key("swagger")
        || compose.additional_entries.contains_key("info")
        || compose.additional_entries.contains_key("paths")
    {
        return false;
    }

    if compose.additional_entries.contains_key("name")
        && (compose.additional_entries.contains_key("on")
            || compose.additional_entries.contains_key("jobs"))
    {
        return false;
    }

    if compose.additional_entries.contains_key("stages")
        || compose.additional_entries.contains_key("pipeline")
        || compose.additional_entries.contains_key("workflows")
    {
        return false;
    }

    true
}

fn should_skip_file(file_name: &str, path: &Path) -> bool {
    if file_name.contains("lock") || file_name.contains("package") {
        return true;
    }

    if file_name.starts_with('.') {
        return true;
    }

    let excluded_patterns = [
        "openapi",
        "swagger",
        "kubernetes",
        "k8s",
        "helm",
        "chart",
        "deployment",
        "service",
        "ingress",
        "config",
        "ci",
        "gitlab",
        "circleci",
        "travis",
        "azure-pipelines",
        "buildkite",
        "cloudbuild",
    ];

    if excluded_patterns
        .iter()
        .any(|pattern| file_name.to_lowercase().contains(pattern))
    {
        return true;
    }

    if let Some(path_str) = path.to_str() {
        let excluded_dirs = [
            ".github",
            ".gitlab",
            ".circleci",
            "kubernetes",
            "k8s",
            "helm",
            ".forklaunch/openapi",
            "docs",
            "documentation",
        ];

        if excluded_dirs.iter().any(|dir| path_str.contains(dir)) {
            return true;
        }
    }

    false
}

fn count_dockerfiles(base_path: &Path) -> usize {
    WalkDir::new(base_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let file_name = e.file_name().to_string_lossy();
            file_name == "Dockerfile"
                || file_name.starts_with("Dockerfile.")
                || file_name.ends_with(".dockerfile")
        })
        .count()
}

pub(crate) fn find_docker_compose_path(base_path: &Path) -> Option<String> {
    let dockerfile_count = count_dockerfiles(base_path);
    if dockerfile_count > 1 {
        return None;
    }

    for entry in WalkDir::new(base_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let file_name = entry.file_name().to_string_lossy();

        if should_skip_file(&file_name, entry.path()) {
            continue;
        }

        if file_name == "docker-compose.yml" || file_name == "docker-compose.yaml" {
            if let Ok(contents) = read_to_string(entry.path()) {
                if let Ok(compose) = from_str::<DockerCompose>(&contents) {
                    if is_valid_docker_compose(&compose) {
                        let relative_path =
                            entry.path().strip_prefix(base_path).unwrap_or(entry.path());
                        return Some(relative_path.to_string_lossy().to_string());
                    }
                }
            }
        }

        if file_name.ends_with(".yml") || file_name.ends_with(".yaml") {
            if let Ok(contents) = read_to_string(entry.path()) {
                if let Ok(compose) = from_str::<DockerCompose>(&contents) {
                    if is_valid_docker_compose(&compose) {
                        let relative_path =
                            entry.path().strip_prefix(base_path).unwrap_or(entry.path());
                        return Some(relative_path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    None
}

const IN_MEMORY_DATABASE_DOCKERFILE_ADDENDUM: &str = "
# Install sqlite dependencies
RUN apk add --no-cache python3 py3-pip make build-base sqlite-dev
RUN pip install setuptools --break-system-packages
";

pub(crate) fn update_dockerfile_contents(
    dockerfile_contents: &str,
    runtime: &Runtime,
    is_in_memory_database: bool,
) -> Result<String> {
    let mut final_copy_line: i32 = -1;
    if runtime == &Runtime::Node
        && is_in_memory_database
        && !dockerfile_contents.contains(IN_MEMORY_DATABASE_DOCKERFILE_ADDENDUM)
    {
        for (index, line) in dockerfile_contents.lines().enumerate() {
            if line.contains("COPY") {
                final_copy_line = index.try_into().unwrap();
                break;
            }
        }
    }

    let mut new_dockerfile_contents = dockerfile_contents.lines().collect::<Vec<&str>>();
    if final_copy_line != -1 {
        new_dockerfile_contents.insert(
            final_copy_line.try_into().unwrap(),
            IN_MEMORY_DATABASE_DOCKERFILE_ADDENDUM,
        );
    }

    Ok(new_dockerfile_contents.join("\n"))
}

/// Synchronize docker-compose environment sections with env vars discovered from code scanning.
///
/// For each service/worker in docker-compose that maps to a known project,
/// adds only the env vars that the project actually uses in its code.
/// New vars are added with empty string values so the user can fill them in.
pub(crate) fn sync_docker_compose_env_vars(
    docker_compose: &mut DockerCompose,
    project_env_vars: &HashMap<String, Vec<super::ast::infrastructure::env::EnvVarUsage>>,
    manifest: &ApplicationManifestData,
    stdout: &mut StandardStream,
) -> Result<bool> {
    let mut changes_made = false;

    let project_types: HashMap<String, super::manifest::ProjectType> = manifest
        .projects
        .iter()
        .map(|p| (p.name.clone(), p.r#type.clone()))
        .collect();

    // For each docker-compose service, figure out which project it corresponds to
    let service_keys: Vec<String> = docker_compose.services.keys().cloned().collect();

    for service_key in &service_keys {
        // Determine the project name this service corresponds to
        let project_name = if project_types.contains_key(service_key) {
            Some(service_key.clone())
        } else {
            // Check worker aliases: "foo-worker" -> "foo", "foo-service" -> "foo"
            let mut found = None;
            for suffix in &["-worker", "-service", "-server"] {
                if let Some(base) = service_key.strip_suffix(suffix) {
                    if project_types.contains_key(base) {
                        found = Some(base.to_string());
                        break;
                    }
                }
            }
            found
        };

        let Some(project_name) = project_name else {
            continue;
        };

        let service = docker_compose.services.get_mut(service_key).unwrap();
        let environment = service.environment.get_or_insert_with(IndexMap::new);

        let mut added_vars = Vec::new();

        // Only add env vars that this project actually uses in its code
        if let Some(env_vars) = project_env_vars.get(&project_name) {
            for env_var in env_vars {
                if !environment.contains_key(&env_var.var_name) {
                    environment.insert(env_var.var_name.clone(), String::new());
                    added_vars.push(env_var.var_name.clone());
                }
            }
        }

        if !added_vars.is_empty() {
            added_vars.sort();
            changes_made = true;
            log_info!(stdout, "Added {} env var(s) to docker-compose service '{}': {}", added_vars.len(), service_key, added_vars.join(", "));
        }
    }

    Ok(changes_made)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::Runtime;

    #[test]
    fn test_update_dockerfile_contents_inserts_addendum_after_last_copy() {
        let dockerfile =
            "FROM node:18-alpine\nCOPY . .\nRUN npm install\nCOPY extra .\nRUN npm run build";
        let runtime = Runtime::Node;
        let is_in_memory_database = true;
        let result =
            update_dockerfile_contents(dockerfile, &runtime, is_in_memory_database).unwrap();
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(
            lines[2],
            IN_MEMORY_DATABASE_DOCKERFILE_ADDENDUM
                .split('\n')
                .collect::<Vec<&str>>()[1]
                .trim()
        );
        assert_eq!(lines[0], "FROM node:18-alpine");
        assert_eq!(lines[6], "COPY . .");
        assert_eq!(lines[7], "RUN npm install");
        assert_eq!(lines[8], "COPY extra .");
        assert_eq!(lines[9], "RUN npm run build");
    }

    #[test]
    fn test_parse_environment_value_rejects_unsupported_types() {
        use serde_yml::Value;

        // Test that Number type is rejected
        let number_value = Value::Number(42.into());
        let result = parse_environment_value(number_value);
        assert!(result.is_err(), "Number type should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("parse_environment_value"),
            "Error should reference parse_environment_value"
        );
        assert!(
            err.to_string().contains("Number"),
            "Error should mention the type"
        );

        // Test that Bool type is rejected
        let bool_value = Value::Bool(true);
        let result = parse_environment_value(bool_value);
        assert!(result.is_err(), "Bool type should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("parse_environment_value"),
            "Error should reference parse_environment_value"
        );
        assert!(
            err.to_string().contains("Bool"),
            "Error should mention the type"
        );
    }

    #[test]
    fn test_parse_environment_value_supports_valid_types() {
        use serde_yml::Value;

        // Test Null
        let result = parse_environment_value(Value::Null);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // Test String with key=value
        let result = parse_environment_value(Value::String("KEY=value".to_string()));
        assert!(result.is_ok());
        let env = result.unwrap().unwrap();
        assert_eq!(env.get("KEY"), Some(&"value".to_string()));

        // Test Sequence
        let seq = Value::Sequence(vec![
            Value::String("KEY1=value1".to_string()),
            Value::String("KEY2=value2".to_string()),
        ]);
        let result = parse_environment_value(seq);
        assert!(result.is_ok());
        let env = result.unwrap().unwrap();
        assert_eq!(env.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(env.get("KEY2"), Some(&"value2".to_string()));

        // Test Mapping
        let mut map = serde_yml::Mapping::new();
        map.insert(
            Value::String("KEY".to_string()),
            Value::String("value".to_string()),
        );
        let result = parse_environment_value(Value::Mapping(map));
        assert!(result.is_ok());
        let env = result.unwrap().unwrap();
        assert_eq!(env.get("KEY"), Some(&"value".to_string()));
    }
}

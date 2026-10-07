use serde::{Deserialize, Serialize};

/// One entry from `GET /platform-resources/application/:applicationId`.
///
/// This mirrors the response body, so it carries fields nothing reads yet.
/// They are kept deliberately: the struct documents the wire contract, and
/// dropping a field here would silently discard it the moment a caller wants
/// it. Serde ignores unknown fields, so their absence would not have failed
/// loudly either.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ResourceListItem {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) r#type: String,
    #[serde(rename = "technology")]
    pub(crate) technology: Option<String>,
    #[serde(rename = "provider")]
    pub(crate) provider: Option<String>,
    #[serde(rename = "serviceId")]
    pub(crate) service_id: Option<String>,
    #[serde(rename = "serviceName")]
    pub(crate) service_name: Option<String>,
    pub(crate) environment: Option<String>,
    pub(crate) region: Option<String>,
    pub(crate) status: String,
    #[serde(rename = "resourceId")]
    pub(crate) resource_id: Option<String>,
    pub(crate) endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ApplicationResourcesResponse {
    pub(crate) resources: Vec<ResourceListItem>,
}

/// `manifestConfig` shape — `ResourceConfigSchema` on the platform. All fields optional;
/// covers both "resize" (instanceClass/allocatedStorage/nodeType/...) and "config-set"
/// (engine/multiAZ/queueType/...) use cases, since the platform makes no distinction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ResourceConfig {
    #[serde(rename = "instanceClass", skip_serializing_if = "Option::is_none")]
    pub(crate) instance_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) engine: Option<String>,
    #[serde(rename = "allocatedStorage", skip_serializing_if = "Option::is_none")]
    pub(crate) allocated_storage: Option<u32>,
    #[serde(rename = "numCacheNodes", skip_serializing_if = "Option::is_none")]
    pub(crate) num_cache_nodes: Option<u32>,
    #[serde(
        rename = "numberOfBrokerNodes",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) number_of_broker_nodes: Option<u32>,
    #[serde(rename = "ebsStorageSize", skip_serializing_if = "Option::is_none")]
    pub(crate) ebs_storage_size: Option<u32>,
    #[serde(rename = "visibilityTimeout", skip_serializing_if = "Option::is_none")]
    pub(crate) visibility_timeout: Option<u32>,
    #[serde(
        rename = "messageRetentionSeconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) message_retention_seconds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) port: Option<u16>,
    #[serde(rename = "multiAZ", skip_serializing_if = "Option::is_none")]
    pub(crate) multi_az: Option<bool>,
    #[serde(rename = "nodeType", skip_serializing_if = "Option::is_none")]
    pub(crate) node_type: Option<String>,
    #[serde(rename = "brokerNodeType", skip_serializing_if = "Option::is_none")]
    pub(crate) broker_node_type: Option<String>,
    #[serde(rename = "kafkaVersion", skip_serializing_if = "Option::is_none")]
    pub(crate) kafka_version: Option<String>,
    #[serde(rename = "queueType", skip_serializing_if = "Option::is_none")]
    pub(crate) queue_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) encryption: Option<String>,
}

/// `GET /platform-resources/:resourceId` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResourceDetailResponse {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) r#type: String,
    pub(crate) provider: String,
    #[serde(rename = "distributionStrategy")]
    pub(crate) distribution_strategy: String,
    pub(crate) status: String,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: String,
    #[serde(rename = "updatedAt")]
    pub(crate) updated_at: String,
    pub(crate) environment: Option<String>,
    pub(crate) region: Option<String>,
    #[serde(rename = "primaryRegion")]
    pub(crate) primary_region: Option<String>,
    #[serde(rename = "resourceId")]
    pub(crate) resource_id: Option<String>,
    pub(crate) arn: Option<String>,
    pub(crate) endpoint: Option<String>,
    #[serde(rename = "serviceId")]
    pub(crate) service_id: Option<String>,
    #[serde(rename = "serviceName")]
    pub(crate) service_name: Option<String>,
    #[serde(rename = "manifestConfig")]
    pub(crate) manifest_config: ResourceConfig,
}

#[derive(Debug, Serialize)]
pub(crate) struct DeployResourceRequest {
    #[serde(rename = "manifestConfig", skip_serializing_if = "Option::is_none")]
    pub(crate) manifest_config: Option<ResourceConfig>,
    #[serde(
        rename = "distributionStrategy",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) distribution_strategy: Option<String>,
    #[serde(rename = "primaryRegion", skip_serializing_if = "Option::is_none")]
    pub(crate) primary_region: Option<String>,
    #[serde(
        rename = "snapshotBeforeChange",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) snapshot_before_change: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeployResourceResponse {
    #[serde(rename = "deploymentId")]
    pub(crate) deployment_id: String,
}

/// `POST /:id/stop` and `POST /:id/delete` both respond with this synchronous shape.
#[derive(Debug, Deserialize)]
pub(crate) struct MessageResponse {
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct PatchResourceRequest {
    #[serde(
        rename = "distributionStrategy",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) distribution_strategy: Option<String>,
    #[serde(rename = "primaryRegion", skip_serializing_if = "Option::is_none")]
    pub(crate) primary_region: Option<String>,
}

/// One CloudWatch time series from `GET /platform-resources/:id/metrics` — e.g.
/// `CPUUtilization`, `FreeableMemory`, `DatabaseConnections` for RDS, or the
/// ElastiCache equivalents. `timestamps`/`values` are parallel arrays over the
/// requested period; `fl infra status --metrics` shows only the latest point,
/// `--json` shows the full series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MetricSeries {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) timestamps: Vec<String>,
    pub(crate) values: Vec<f64>,
}

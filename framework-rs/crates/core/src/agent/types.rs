//! Wire shapes of `/api/v1/agent`, shared with the TypeScript framework.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP tool annotations. Unset hints are omitted from JSON.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

impl ToolAnnotations {
    /// `{ readOnlyHint: true }`: the only annotation that runs without approval.
    #[must_use]
    pub fn read_only() -> Self {
        Self {
            read_only_hint: Some(true),
            ..Self::default()
        }
    }

    /// Only tools that declare themselves read-only run without the user's approval.
    #[must_use]
    pub fn requires_approval(&self) -> bool {
        self.read_only_hint != Some(true)
    }

    /// Fills hints this value leaves unset from `defaults`.
    #[must_use]
    pub fn or(self, defaults: Self) -> Self {
        Self {
            title: self.title.or(defaults.title),
            read_only_hint: self.read_only_hint.or(defaults.read_only_hint),
            destructive_hint: self.destructive_hint.or(defaults.destructive_hint),
            idempotent_hint: self.idempotent_hint.or(defaults.idempotent_hint),
            open_world_hint: self.open_world_hint.or(defaults.open_world_hint),
        }
    }
}

/// Where a tool comes from: the agent's code, an app service or an MCP server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ToolSourceKind {
    Local,
    Service,
    Mcp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolSourceRef {
    pub kind: ToolSourceKind,
    pub name: String,
}

/// One entry of `GET /api/v1/agent/tools`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListedTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    pub input_schema: Value,
    pub annotations: ToolAnnotations,
    pub source: ToolSourceRef,
    pub requires_approval: bool,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

/// Body of `GET /api/v1/agent/tools`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentToolListing {
    pub agent: String,
    pub model: Option<String>,
    pub tools: Vec<ListedTool>,
}

/// The user's answer to an `approval_required` event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Approve,
    Deny,
}

/// Body of the task routes: start a task, or resume one after an approval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AgentTaskRequest {
    Resume {
        continuation: String,
        decision: ApprovalDecision,
    },
    Start {
        task: String,
    },
}

/// Fields of an `approval_required` event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalRequired {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub annotations: ToolAnnotations,
    pub continuation: String,
}

/// Events of `POST /api/v1/agent/tasks/stream`; one JSON object per `data:` line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Status {
        message: String,
    },
    Delta {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        id: String,
        ok: bool,
        summary: String,
    },
    ApprovalRequired(ApprovalRequired),
    Done {
        model: String,
    },
    Error {
        message: String,
    },
}

/// The `approval` field of a task result: the `approval_required` event itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PendingApproval {
    ApprovalRequired(ApprovalRequired),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCallRecord {
    pub id: String,
    pub name: String,
    pub ok: bool,
}

/// Body of `POST /api/v1/agent/tasks`. `answer` is null while an approval is pending.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskResult {
    pub answer: Option<String>,
    pub model: String,
    pub tool_calls: Vec<ToolCallRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<PendingApproval>,
}

//! Encrypted, tenant-bound jobs. PostgreSQL owns dispatch atomicity; the API owns
//! result idempotence and revision CAS. Neither the queue nor retries confer note access.
use uuid::Uuid;
use zeroize::Zeroize;

pub const QUEUE: &str = "fieldnotes-note-insights-v2";
pub const PURPOSE: &str = "note-insight-job-v2";

#[framework_core::contract::schema(rename_all = "camelCase", unknown_fields = "deny")]
#[derive(Clone)]
pub struct SealedNoteJob {
    pub operation_id: Uuid,
    pub workspace_id: Uuid,
    pub note_id: Uuid,
    #[field(min = 1)]
    pub revision: i64,
    pub payload: String,
}

#[framework_core::contract::schema(rename_all = "camelCase", unknown_fields = "deny")]

pub struct NoteJob {
    pub operation_id: Uuid,
    pub workspace_id: Uuid,
    pub note_id: Uuid,
    #[field(min = 1)]
    pub revision: i64,
    pub body: String,
}

impl SealedNoteJob {
    pub fn context(&self) -> ::worker::JobContext {
        ::worker::JobContext {
            tenant_id: self.workspace_id,
            resource_id: self.note_id,
            operation_id: self.operation_id,
            revision: self.revision,
        }
    }
}
impl Zeroize for NoteJob {
    fn zeroize(&mut self) {
        self.body.zeroize();
    }
}
impl ::worker::JobPayload for NoteJob {
    fn context(&self) -> ::worker::JobContext {
        ::worker::JobContext {
            tenant_id: self.workspace_id,
            resource_id: self.note_id,
            operation_id: self.operation_id,
            revision: self.revision,
        }
    }
}

/// Encrypted worker result. This contract is shared by the publisher and API.
pub const RESULT_PURPOSE: &str = "note-insight-result-v1";
#[framework_core::contract::schema(rename_all = "camelCase", unknown_fields = "deny")]

pub struct InsightEvent {
    pub operation_id: Uuid,
    pub workspace_id: Uuid,
    pub note_id: Uuid,
    #[field(min = 1)]
    pub revision: i64,
    pub excerpt: String,
    #[field(min = 0)]
    pub word_count: u64,
}
impl Zeroize for InsightEvent {
    fn zeroize(&mut self) {
        self.excerpt.zeroize();
    }
}
impl worker::JobPayload for InsightEvent {
    fn context(&self) -> worker::JobContext {
        worker::JobContext {
            tenant_id: self.workspace_id,
            resource_id: self.note_id,
            operation_id: self.operation_id,
            revision: self.revision,
        }
    }
}

/// Version 2 job plaintext: the trace identity is encrypted with the domain payload.
pub type TracedNoteJob = ::worker::TracedJob<NoteJob>;

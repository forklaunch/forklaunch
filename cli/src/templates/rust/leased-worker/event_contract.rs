use forklaunch_worker_interface::{JobContext, JobPayload};
use uuid::Uuid;
use zeroize::Zeroize;

#[forklaunch_core::contract::schema(rename_all = "camelCase", unknown_fields = "deny")]
pub struct WorkInput {
    pub resource_id: Uuid,
    #[field(min = 1, max = 9_007_199_254_740_991)]
    pub revision: i64,
    pub message: String,
}
impl Zeroize for WorkInput {
    fn zeroize(&mut self) {
        self.message.zeroize();
    }
}

#[forklaunch_core::contract::schema(rename_all = "camelCase", unknown_fields = "deny")]
pub struct WorkEvent {
    pub context: JobContext,
    pub queue: String,
    pub input: WorkInput,
}
impl Zeroize for WorkEvent {
    fn zeroize(&mut self) {
        self.input.zeroize();
    }
}
impl JobPayload for WorkEvent {
    fn context(&self) -> JobContext {
        self.context
    }
}


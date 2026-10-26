use crate::{WORKER_NAME, WorkerContext, error::Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReport {
    pub worker: String,
    pub processed: usize,
    pub message: String,
}

#[must_use]
pub fn pending_jobs(context: &WorkerContext) -> Vec<Job> {
    (0..context.config().batch_size)
        .map(|index| Job {
            id: format!("{}-{index}", context.config().queue_name),
            payload: format!("{} payload {index}", context.config().worker_type),
        })
        .collect()
}

pub fn process_next(context: &WorkerContext) -> Result<JobReport> {
    let jobs = pending_jobs(context);
    Ok(JobReport {
        worker: WORKER_NAME.to_string(),
        processed: jobs.len(),
        message: format!("processed {} job(s) from {}", jobs.len(), context.config().queue_name),
    })
}

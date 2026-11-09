#![forbid(unsafe_code)]

#[cfg(feature = "redis-client")]
pub mod leased;
#[cfg(feature = "redis-client")]
pub mod signals;
#[cfg(feature = "redis-client")]
pub mod tenant_isolation;

use std::{collections::VecDeque, sync::Mutex};

use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_worker_interface::{Job, JobReceipt, QueueStats, WorkerQueue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedisWorkerQueueConfig {
    pub queue_prefix: String,
}

#[derive(Debug)]
pub struct RedisWorkerQueue {
    pub config: RedisWorkerQueueConfig,
    queued: Mutex<VecDeque<Job>>,
    processing: Mutex<Vec<Job>>,
}

impl RedisWorkerQueue {
    #[must_use]
    pub fn new(config: RedisWorkerQueueConfig) -> Self {
        Self {
            config,
            queued: Mutex::new(VecDeque::new()),
            processing: Mutex::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn queue_key(&self, queue: &str) -> String {
        format!("{}:{queue}", self.config.queue_prefix)
    }
}

impl WorkerQueue for RedisWorkerQueue {
    fn enqueue(&self, job: Job) -> BlueprintResult<JobReceipt> {
        let receipt = JobReceipt {
            id: job.id.clone(),
            queue: job.queue.clone(),
        };
        self.queued
            .lock()
            .map_err(|_| queue_error("queue lock poisoned"))?
            .push_back(job);
        Ok(receipt)
    }

    fn dequeue(&self, queue: &str) -> BlueprintResult<Option<Job>> {
        let mut queued = self
            .queued
            .lock()
            .map_err(|_| queue_error("queue lock poisoned"))?;
        let Some(index) = queued.iter().position(|job| job.queue == queue) else {
            return Ok(None);
        };
        let job = queued.remove(index).expect("index was checked");
        self.processing
            .lock()
            .map_err(|_| queue_error("processing lock poisoned"))?
            .push(job.clone());
        Ok(Some(job))
    }

    fn ack(&self, job_id: &str) -> BlueprintResult<()> {
        self.processing
            .lock()
            .map_err(|_| queue_error("processing lock poisoned"))?
            .retain(|job| job.id != job_id);
        Ok(())
    }

    fn fail(&self, job_id: &str, _reason: &str) -> BlueprintResult<()> {
        self.ack(job_id)
    }

    fn stats(&self, queue: &str) -> BlueprintResult<QueueStats> {
        let queued = self
            .queued
            .lock()
            .map_err(|_| queue_error("queue lock poisoned"))?
            .iter()
            .filter(|job| job.queue == queue)
            .count();
        let processing = self
            .processing
            .lock()
            .map_err(|_| queue_error("processing lock poisoned"))?
            .iter()
            .filter(|job| job.queue == queue)
            .count();

        Ok(QueueStats {
            queue: queue.to_owned(),
            queued,
            processing,
        })
    }
}

fn queue_error(message: &str) -> BlueprintError {
    BlueprintError::new("redis_worker_queue_error", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_queue_uses_prefix_and_processes_jobs() {
        let queue = RedisWorkerQueue::new(RedisWorkerQueueConfig {
            queue_prefix: "forklaunch".to_owned(),
        });

        queue
            .enqueue(Job::new("job_1", "email", "{}"))
            .expect("enqueue should succeed");

        assert_eq!(queue.queue_key("email"), "forklaunch:email");
        assert!(
            queue
                .dequeue("email")
                .expect("dequeue should succeed")
                .is_some()
        );
    }
}

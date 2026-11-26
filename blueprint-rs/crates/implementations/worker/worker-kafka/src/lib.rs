//! Kafka worker adapters. `KafkaWorkerQueue` is an in-memory reference only.
//! Enable `kafka-client` for broker-backed transactional clients in `client`.
#![forbid(unsafe_code)]

#[cfg(feature = "kafka-client")]
pub mod client;

use std::{collections::VecDeque, sync::Mutex};

use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_worker_interface::{Job, JobReceipt, QueueStats, WorkerQueue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KafkaWorkerQueueConfig {
    pub topic: String,
}

#[derive(Debug)]
pub struct KafkaWorkerQueue {
    pub config: KafkaWorkerQueueConfig,
    queued: Mutex<VecDeque<Job>>,
    processing: Mutex<Vec<Job>>,
}

impl KafkaWorkerQueue {
    #[must_use]
    pub fn new(config: KafkaWorkerQueueConfig) -> Self {
        Self {
            config,
            queued: Mutex::new(VecDeque::new()),
            processing: Mutex::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn topic_for_queue(&self, queue: &str) -> String {
        format!("{}.{}", self.config.topic, queue)
    }
}

impl WorkerQueue for KafkaWorkerQueue {
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
    BlueprintError::new("kafka_worker_queue_error", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kafka_queue_maps_queue_names_to_topics() {
        let queue = KafkaWorkerQueue::new(KafkaWorkerQueueConfig {
            topic: "forklaunch.jobs".to_owned(),
        });

        queue
            .enqueue(Job::new("job_1", "email", "{}"))
            .expect("enqueue should succeed");

        assert_eq!(queue.topic_for_queue("email"), "forklaunch.jobs.email");
        assert_eq!(queue.stats("email").expect("stats should load").queued, 1);
    }
}

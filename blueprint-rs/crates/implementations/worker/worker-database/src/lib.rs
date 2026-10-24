#![forbid(unsafe_code)]

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "http-control")]
pub mod http;

#[cfg(feature = "runtime")]
pub mod control;

#[cfg(feature = "runtime")]
pub mod runner;

#[cfg(feature = "runtime")]
pub mod retention;

use std::{collections::VecDeque, sync::Mutex};

use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_worker_interface::{
    DistributedSignal, Job, JobReceipt, QueueStats, SignalBus, WorkerQueue,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseWorkerQueueConfig {
    pub table_name: String,
}

#[derive(Debug)]
pub struct DatabaseWorkerQueue {
    pub config: DatabaseWorkerQueueConfig,
    queued: Mutex<VecDeque<Job>>,
    processing: Mutex<Vec<Job>>,
    failures: Mutex<Vec<(String, String)>>,
}

impl DatabaseWorkerQueue {
    #[must_use]
    pub fn new(config: DatabaseWorkerQueueConfig) -> Self {
        Self {
            config,
            queued: Mutex::new(VecDeque::new()),
            processing: Mutex::new(Vec::new()),
            failures: Mutex::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn failures(&self) -> Vec<(String, String)> {
        self.failures
            .lock()
            .map_or_else(|_| Vec::new(), |failures| failures.clone())
    }
}

impl WorkerQueue for DatabaseWorkerQueue {
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

    fn fail(&self, job_id: &str, reason: &str) -> BlueprintResult<()> {
        self.ack(job_id)?;
        self.failures
            .lock()
            .map_err(|_| queue_error("failures lock poisoned"))?
            .push((job_id.to_owned(), reason.to_owned()));
        Ok(())
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
    BlueprintError::new("database_worker_queue_error", message)
}

/// An in-memory [`SignalBus`] for broadcasting control signals to workers.
#[derive(Debug, Default)]
pub struct InMemorySignalBus {
    signals: Mutex<Vec<DistributedSignal>>,
}

impl InMemorySignalBus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SignalBus for InMemorySignalBus {
    fn broadcast(&self, signal: DistributedSignal) -> BlueprintResult<()> {
        self.signals
            .lock()
            .map_err(|_| queue_error("signal lock poisoned"))?
            .push(signal);
        Ok(())
    }

    fn drain(&self) -> BlueprintResult<Vec<DistributedSignal>> {
        let mut signals = self
            .signals
            .lock()
            .map_err(|_| queue_error("signal lock poisoned"))?;
        Ok(std::mem::take(&mut *signals))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_queue_enqueues_dequeues_and_acks_jobs() {
        let queue = DatabaseWorkerQueue::new(DatabaseWorkerQueueConfig {
            table_name: "jobs".to_owned(),
        });

        queue
            .enqueue(Job::new("job_1", "email", "{}"))
            .expect("enqueue should succeed");
        assert_eq!(queue.stats("email").expect("stats should load").queued, 1);

        let job = queue
            .dequeue("email")
            .expect("dequeue should succeed")
            .expect("job should exist");
        queue.ack(&job.id).expect("ack should succeed");

        assert_eq!(
            queue.stats("email").expect("stats should load").processing,
            0
        );
    }

    #[test]
    fn signal_bus_broadcasts_and_drains_signals() {
        let bus = InMemorySignalBus::new();
        bus.broadcast(DistributedSignal::Pause)
            .expect("broadcast should succeed");
        bus.broadcast(DistributedSignal::Drain)
            .expect("broadcast should succeed");

        let signals = bus.drain().expect("drain should succeed");
        assert_eq!(
            signals,
            vec![DistributedSignal::Pause, DistributedSignal::Drain]
        );
        // draining is idempotent — pending signals are consumed
        assert!(bus.drain().expect("drain should succeed").is_empty());
    }
}

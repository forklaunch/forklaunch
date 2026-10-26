//! Process-local controls shared by all consumers of one worker runner.
use forklaunch_core::domain::BlueprintResult;
use forklaunch_worker_interface::{AsyncSignalBus, DistributedSignal};
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    Running,
    Paused,
    Draining,
}

/// Clones address the same running process. Draining is irreversible; a restart
/// requires a new control. Pause stops future claims, while active handlers and
/// already submitted claims continue with lease renewal.
#[derive(Debug, Clone)]
pub struct WorkerControl(watch::Sender<WorkerState>);
impl Default for WorkerControl {
    fn default() -> Self {
        Self::new()
    }
}
impl WorkerControl {
    #[must_use]
    pub fn new() -> Self {
        Self(watch::channel(WorkerState::Running).0)
    }
    #[must_use]
    pub fn state(&self) -> WorkerState {
        *self.0.borrow()
    }
    /// Returns false after drain has started. Repeated pauses succeed.
    #[must_use]
    pub fn pause(&self) -> bool {
        self.set(WorkerState::Paused)
    }
    /// Returns false after drain has started. Repeated resumes succeed.
    #[must_use]
    pub fn resume(&self) -> bool {
        self.set(WorkerState::Running)
    }
    pub fn drain(&self) {
        self.set(WorkerState::Draining);
    }
    /// Applies a distributed signal using the same irreversible drain rules.
    #[must_use]
    pub fn apply_signal(&self, signal: DistributedSignal) -> bool {
        self.set(match signal {
            DistributedSignal::Pause => WorkerState::Paused,
            DistributedSignal::Resume => WorkerState::Running,
            DistributedSignal::Drain => WorkerState::Draining,
        })
    }
    fn set(&self, next: WorkerState) -> bool {
        let mut accepted = false;
        self.0.send_if_modified(|state| {
            if *state == WorkerState::Draining {
                return false;
            }
            accepted = true;
            if *state == next {
                return false;
            }
            *state = next;
            true
        });
        accepted
    }
    /// Completes immediately if drain has already started.
    pub async fn draining(&self) {
        let mut receiver = self.0.subscribe();
        loop {
            if *receiver.borrow_and_update() == WorkerState::Draining {
                return;
            }
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
    /// Waits until the current state differs from `previous`, including transitions
    /// that happened before this future was polled.
    pub async fn changed(&self, previous: WorkerState) -> WorkerState {
        let mut receiver = self.0.subscribe();
        loop {
            let state = *receiver.borrow_and_update();
            if state != previous {
                return state;
            }
            if receiver.changed().await.is_err() {
                return state;
            }
        }
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<WorkerState> {
        self.0.subscribe()
    }
}

/// Forwards distributed signals to a runner's shared process control. Subscription
/// failure initiates graceful drain so the runner cannot keep claiming work with
/// an unhealthy control channel. Publications are not durable desired state.
/// # Errors
/// Returns the bus error after initiating drain; callers may reconnect/restart.
pub async fn forward_signals<B, F>(
    bus: &mut B,
    control: &WorkerControl,
    shutdown: F,
) -> BlueprintResult<()>
where
    B: AsyncSignalBus,
    F: std::future::Future<Output = ()>,
{
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => { control.drain(); return Ok(()); }
            () = control.draining() => return Ok(()),
            signal = bus.next_signal(std::time::Duration::from_secs(30)) => {
                match signal {
                    Ok(Some(signal)) => { let _ = control.apply_signal(signal); }
                    Ok(None) => (),
                    Err(error) => { control.drain(); return Err(error); }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distributed_control_keeps_drain_irreversible() {
        let control = WorkerControl::new();
        assert!(control.apply_signal(DistributedSignal::Pause));
        assert_eq!(control.state(), WorkerState::Paused);
        assert!(control.apply_signal(DistributedSignal::Resume));
        assert_eq!(control.state(), WorkerState::Running);
        assert!(control.apply_signal(DistributedSignal::Drain));
        assert!(!control.apply_signal(DistributedSignal::Resume));
        assert_eq!(control.state(), WorkerState::Draining);
    }
    #[tokio::test]
    async fn change_waiter_observes_existing_and_future_transitions() {
        let control = WorkerControl::new();
        assert!(control.pause());
        assert_eq!(
            control.changed(WorkerState::Running).await,
            WorkerState::Paused
        );
        let waiting = control.changed(WorkerState::Paused);
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), &mut waiting)
                .await
                .is_err()
        );
        control.drain();
        assert_eq!(waiting.await, WorkerState::Draining);
        assert!(!control.resume());
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(1),
                control.changed(WorkerState::Draining)
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn control_retains_state_without_receivers_and_drain_is_sticky() {
        let control = WorkerControl::new();
        assert!(control.pause());
        let cloned = control.clone();
        assert_eq!(cloned.state(), WorkerState::Paused);
        assert!(cloned.resume());
        assert_eq!(control.state(), WorkerState::Running);
        let wait = control.draining();
        tokio::pin!(wait);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), &mut wait)
                .await
                .is_err()
        );
        cloned.drain();
        wait.await;
        assert!(!control.resume());
        assert!(!cloned.pause());
        control.drain();
        assert_eq!(control.state(), WorkerState::Draining);
        control.draining().await;
    }
}

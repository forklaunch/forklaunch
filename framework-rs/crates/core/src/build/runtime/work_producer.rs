/// The producer always authenticates and encrypts before returning transport data.
pub fn seal_work(
    key: &forklaunch_core::encryption::EnvelopeKey,
    tenant: Uuid,
    queue: &str,
    operation: Uuid,
    input: WorkInput,
) -> Result<forklaunch_worker_interface::SealedEvent, forklaunch_worker_interface::ProcessorError> {
    let event = zeroize::Zeroizing::new(WorkEvent {
        context: JobContext {
            tenant_id: tenant,
            resource_id: input.resource_id,
            operation_id: operation,
            revision: input.revision,
        },
        queue: queue.into(),
        input,
    });
    forklaunch_worker_interface::EncryptedChannel::new("work-v1")?.seal(key, tenant, &*event)
}

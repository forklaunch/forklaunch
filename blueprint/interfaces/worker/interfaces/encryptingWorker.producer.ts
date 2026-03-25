import type { WorkerEventEntity } from '../types/workerEventEntity.types';
import type {
  EventEncryptor,
  EncryptedEventEnvelope
} from '../types/eventEncryptor.types';
import type { WorkerProducer } from './worker.producer.interface';

/**
 * Wraps any WorkerProducer to encrypt the entire event payload before enqueueing.
 * Base WorkerEventEntity fields (id, retryCount, processed, createdAt, updatedAt)
 * are preserved in the clear for queue infrastructure. All remaining fields are
 * JSON-serialized and encrypted into a single `encryptedPayload` field.
 *
 * tenantId is injected at construction (from request scope context) and stored
 * on the envelope for key derivation during decryption.
 */
export class EncryptingWorkerProducer<T extends WorkerEventEntity>
  implements WorkerProducer<T>
{
  constructor(
    private readonly inner: WorkerProducer<EncryptedEventEnvelope>,
    private readonly encryptor: EventEncryptor,
    private readonly tenantId: string
  ) {}

  async enqueueJob(job: T): Promise<void> {
    await this.inner.enqueueJob(
      encryptEvent(job, this.encryptor, this.tenantId)
    );
  }

  async enqueueBatchJobs(jobs: T[]): Promise<void> {
    await this.inner.enqueueBatchJobs(
      jobs.map((job) => encryptEvent(job, this.encryptor, this.tenantId))
    );
  }
}

function encryptEvent<T extends WorkerEventEntity>(
  event: T,
  encryptor: EventEncryptor,
  tenantId: string
): EncryptedEventEnvelope {
  const { id, retryCount, processed, createdAt, updatedAt, ...payload } =
    event as WorkerEventEntity & Record<string, unknown>;

  return {
    id,
    tenantId,
    retryCount,
    processed,
    createdAt,
    updatedAt,
    encryptedPayload: encryptor.encrypt(JSON.stringify(payload), tenantId)!
  };
}

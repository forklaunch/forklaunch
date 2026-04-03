import type { WorkerEventEntity } from '../types/workerEventEntity.types';
import type {
  WorkerProcessFunction,
  WorkerProcessFailureResult,
  WorkerFailureHandler
} from '../types/worker.consumer.types';
import type {
  EventEncryptor,
  EncryptedEventEnvelope
} from '../types/eventEncryptor.types';
import type { WorkerConsumer } from './worker.consumer.interface';

/**
 * Wraps a WorkerConsumer that reads EncryptedEventEnvelope and exposes
 * the decrypted user-typed event T.
 */
export class DecryptingWorkerConsumer<T extends WorkerEventEntity>
  implements WorkerConsumer<T>
{
  constructor(
    private readonly inner: WorkerConsumer<EncryptedEventEnvelope>,
    private readonly encryptor: EventEncryptor
  ) {}

  async peekEvents(): Promise<T[]> {
    const events = await this.inner.peekEvents();
    return events.map((event) => decryptEvent<T>(event, this.encryptor));
  }

  async start(): Promise<void> {
    await this.inner.start();
  }
}

/**
 * Wraps a user-typed WorkerProcessFunction to accept EncryptedEventEnvelope,
 * decrypt before processing, and map failures back to the envelope type.
 */
export function withDecryption<T extends WorkerEventEntity>(
  processFn: WorkerProcessFunction<T>,
  encryptor: EventEncryptor
): WorkerProcessFunction<EncryptedEventEnvelope> {
  return async (events: EncryptedEventEnvelope[]) => {
    const decrypted = events.map((e) => decryptEvent<T>(e, encryptor));
    const failures = await processFn(decrypted);
    // Map failures back: find the original envelope by id
    return failures.map((f) => ({
      value: events.find((e) => e.id === f.value.id)!,
      error: f.error
    }));
  };
}

/**
 * Wraps a user-typed WorkerFailureHandler to accept EncryptedEventEnvelope failures,
 * decrypting before passing to the user handler.
 */
export function withDecryptionFailureHandler<T extends WorkerEventEntity>(
  handler: WorkerFailureHandler<T>,
  encryptor: EventEncryptor
): WorkerFailureHandler<EncryptedEventEnvelope> {
  return async (
    results: WorkerProcessFailureResult<EncryptedEventEnvelope>[]
  ) => {
    const decrypted = results.map((r) => ({
      ...r,
      value: decryptEvent<T>(r.value, encryptor)
    }));
    return handler(decrypted);
  };
}

function decryptEvent<T extends WorkerEventEntity>(
  envelope: EncryptedEventEnvelope,
  encryptor: EventEncryptor
): T {
  const {
    id,
    tenantId,
    retryCount,
    processed,
    createdAt,
    updatedAt,
    encryptedPayload
  } = envelope;

  const decrypted = encryptor.decrypt(encryptedPayload, tenantId);
  const payload = JSON.parse(decrypted!);

  return {
    id,
    tenantId,
    retryCount,
    processed,
    createdAt,
    updatedAt,
    ...payload
  } as T;
}

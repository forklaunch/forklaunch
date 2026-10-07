import type { WorkerEventEntity } from './workerEventEntity.types';

/**
 * Minimal encryptor interface for worker event payload encryption.
 * Keeps interfaces-worker free of @forklaunch/core dependency.
 * Implement with FieldEncryptor from @forklaunch/core/persistence.
 */
export interface EventEncryptor {
  encrypt(plaintext: string | null, tenantId: string): string | null;
  decrypt(ciphertext: string | null, tenantId: string): string | null;
}

/**
 * The on-wire shape stored in the queue. Base fields are in the clear
 * for queue infrastructure (retry logic, dedup, etc.). The user's
 * payload fields are serialized and encrypted into `encryptedPayload`.
 */
export type EncryptedEventEnvelope = WorkerEventEntity & {
  tenantId: string;
  encryptedPayload: string;
};

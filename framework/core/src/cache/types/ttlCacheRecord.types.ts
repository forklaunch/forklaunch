/**
 * Type representing a TTL (Time-To-Live) cache record.
 *
 * @typedef {Object} TtlCacheRecord
 * @property {string} key - The key of the cache record.
 * @property {any} value - The value of the cache record.
 * @property {number} ttlMilliseconds - The time-to-live of the cache record in milliseconds.
 */
export type TtlCacheRecord<T> = {
  key: string;
  value: T;
  ttlMilliseconds: number;
};

/**
 * Optional compliance context for operations that require tenant-scoped encryption.
 * When provided, values are encrypted/decrypted using the tenantId for key derivation.
 * When omitted, values are stored/read as plaintext.
 *
 * Used by both TtlCache (Redis) and ObjectStore (S3).
 */
export interface ComplianceContext {
  tenantId: string;
}

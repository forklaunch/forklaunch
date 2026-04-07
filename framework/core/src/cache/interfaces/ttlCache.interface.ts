import {
  ComplianceContext,
  TtlCacheRecord
} from '../types/ttlCacheRecord.types';

/**
 * Interface representing a TTL (Time-To-Live) cache.
 *
 * Methods that read or write values accept an optional `compliance` parameter.
 * When provided, values are encrypted on write and decrypted on read using
 * the tenant ID for key derivation. When omitted, values are stored as plaintext.
 */
export interface TtlCache {
  /**
   * Puts a record into the cache.
   */
  putRecord<T>(
    cacheRecord: TtlCacheRecord<T>,
    compliance?: ComplianceContext
  ): Promise<void>;

  /**
   * Puts a batch of records into the cache.
   */
  putBatchRecords<T>(
    cacheRecords: TtlCacheRecord<T>[],
    compliance?: ComplianceContext
  ): Promise<void>;

  /**
   * Enqueues a record into a list.
   */
  enqueueRecord<T>(
    queueName: string,
    cacheRecord: T,
    compliance?: ComplianceContext
  ): Promise<void>;

  /**
   * Enqueues a batch of records into a list.
   */
  enqueueBatchRecords<T>(
    queueName: string,
    cacheRecords: T[],
    compliance?: ComplianceContext
  ): Promise<void>;

  /**
   * Deletes a record from the cache.
   */
  deleteRecord(cacheRecordKey: string): Promise<void>;

  /**
   * Deletes a batch of records from the cache.
   */
  deleteBatchRecords(cacheRecordKeys: string[]): Promise<void>;

  /**
   * Dequeues a record from a list.
   */
  dequeueRecord<T>(
    queueName: string,
    compliance?: ComplianceContext
  ): Promise<T>;

  /**
   * Dequeues a batch of records from a list.
   */
  dequeueBatchRecords<T>(
    queueName: string,
    pageSize: number,
    compliance?: ComplianceContext
  ): Promise<T[]>;

  /**
   * Reads a record from the cache.
   */
  readRecord<T>(
    cacheRecordKey: string,
    compliance?: ComplianceContext
  ): Promise<TtlCacheRecord<T>>;

  /**
   * Reads a batch of records from the cache.
   */
  readBatchRecords<T>(
    cacheRecordKeysOrPrefix: string[] | string,
    compliance?: ComplianceContext
  ): Promise<TtlCacheRecord<T>[]>;

  /**
   * Checks if a record exists in the cache.
   */
  peekRecord(cacheRecordKey: string): Promise<boolean>;

  /**
   * Checks if a batch of records exist in the cache.
   */
  peekBatchRecords(
    cacheRecordKeysOrPrefix: string[] | string
  ): Promise<boolean[]>;

  /**
   * Peeks at the front of a queue without removing.
   */
  peekQueueRecord<T>(
    queueName: string,
    compliance?: ComplianceContext
  ): Promise<T>;

  /**
   * Peeks at multiple records from a queue without removing.
   */
  peekQueueRecords<T>(
    queueName: string,
    pageSize: number,
    compliance?: ComplianceContext
  ): Promise<T[]>;

  /**
   * Gets the default TTL in milliseconds.
   */
  getTtlMilliseconds(): number;

  /**
   * Lists keys matching a prefix.
   */
  listKeys(pattern_prefix: string): Promise<string[]>;
}

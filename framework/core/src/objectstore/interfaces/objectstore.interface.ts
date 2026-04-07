import { Readable } from 'stream';
import type { ComplianceContext } from '../../cache/types/ttlCacheRecord.types';

/**
 * Interface representing an object store.
 *
 * Methods that read or write object bodies accept an optional `compliance` parameter.
 * When provided, bodies are encrypted on write and decrypted on read using
 * the tenant ID for key derivation. When omitted, bodies are stored as plaintext.
 */
export interface ObjectStore<Client> {
  /**
   * Puts a record into the objectstore.
   */
  putObject<T>(object: T, compliance?: ComplianceContext): Promise<void>;

  /**
   * Puts a batch of records into the objectstore.
   */
  putBatchObjects<T>(
    objects: T[],
    compliance?: ComplianceContext
  ): Promise<void>;

  /**
   * Streams an object upload to the objectstore.
   */
  streamUploadObject<T>(
    object: T,
    compliance?: ComplianceContext
  ): Promise<void>;

  /**
   * Streams a batch of object uploads to the objectstore.
   */
  streamUploadBatchObjects<T>(
    objects: T[],
    compliance?: ComplianceContext
  ): Promise<void>;

  /**
   * Deletes a record from the objectstore.
   */
  deleteObject(objectKey: string): Promise<void>;

  /**
   * Deletes a batch of records from the objectstore.
   */
  deleteBatchObjects(objectKeys: string[]): Promise<void>;

  /**
   * Reads a record from the objectstore.
   */
  readObject<T>(objectKey: string, compliance?: ComplianceContext): Promise<T>;

  /**
   * Reads a batch of records from the objectstore.
   */
  readBatchObjects<T>(
    objectKeys: string[],
    compliance?: ComplianceContext
  ): Promise<T[]>;

  /**
   * Streams a download from the objectstore.
   * Note: Streaming bypasses application-level encryption/decryption.
   */
  streamDownloadObject(objectKey: string): Promise<Readable>;

  /**
   * Streams multiple downloads from the objectstore.
   * Note: Streaming bypasses application-level encryption/decryption.
   */
  streamDownloadBatchObjects(objectKeys: string[]): Promise<Readable[]>;

  /**
   * Gets the underlying objectstore client instance.
   */
  getClient(): Client;
}

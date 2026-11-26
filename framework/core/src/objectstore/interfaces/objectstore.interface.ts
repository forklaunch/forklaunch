import { Readable } from 'stream';
import type { ComplianceContext } from '../../cache/types/ttlCacheRecord.types';

/** A file body: bytes, text or a stream. */
export type ObjectStoreFileBody = Buffer | Uint8Array | string | Readable;

export interface PutFileOptions {
  contentType: string;
  /** Sets Content-Disposition, so a download saves under this name. */
  filename?: string;
  metadata?: Record<string, string>;
}

export interface PresignUploadOptions {
  /** The only content type the upload may carry. */
  contentType: string;
  /** Largest accepted upload, in bytes; the store rejects anything bigger. */
  maxBytes: number;
  /** Link lifetime in seconds. Default and ceiling: the store's upload cap. */
  expiresIn?: number;
}

/**
 * A browser upload grant: a form POST of `fields`, then the file as the last
 * field (named `file`), to `url`.
 */
export interface PresignedUpload {
  url: string;
  fields: Record<string, string>;
  /** The key the object will be stored under, relative to the store. */
  key: string;
  expiresAt: Date;
}

export interface PresignDownloadOptions {
  /** Link lifetime in seconds. Default and ceiling: the store's download cap. */
  expiresIn?: number;
  /** Serve as an attachment saved under this name. */
  filename?: string;
}

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
   * Stores a file as-is (not JSON-encoded, not encrypted by the app; the
   * store's own at-rest encryption applies).
   */
  putFile(
    objectKey: string,
    body: ObjectStoreFileBody,
    options: PutFileOptions
  ): Promise<void>;

  /**
   * A short-lived grant for a browser to upload one file directly, with the
   * size limit and content type enforced by the store.
   */
  presignUpload(
    objectKey: string,
    options: PresignUploadOptions
  ): Promise<PresignedUpload>;

  /** A short-lived link to download one object. */
  presignDownload(
    objectKey: string,
    options?: PresignDownloadOptions
  ): Promise<string>;

  /**
   * Gets the underlying objectstore client instance.
   */
  getClient(): Client;
}

import {
  CreateBucketCommand,
  DeleteObjectCommand,
  DeleteObjectsCommand,
  DeleteObjectsCommandInput,
  GetObjectCommand,
  HeadBucketCommand,
  PutObjectCommand,
  PutObjectCommandInput,
  S3Client
} from '@aws-sdk/client-s3';
import {
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { type FieldEncryptor } from '@forklaunch/core/persistence';
import { ObjectStore } from '@forklaunch/core/objectstore';
import type { ComplianceContext } from '@forklaunch/core/cache';
import { Readable } from 'stream';

const ENCRYPTED_PREFIXES = ['v1:', 'v2:'] as const;

function isEncrypted(value: string): boolean {
  return ENCRYPTED_PREFIXES.some((p) => value.startsWith(p));
}

/**
 * Options for configuring encryption on the S3 object store.
 * Required — every consumer must explicitly configure encryption.
 */
export interface S3EncryptionOptions {
  /** The FieldEncryptor instance to use for encrypting object bodies. */
  encryptor: FieldEncryptor;
}

/**
 * Options for configuring the S3ObjectStore.
 */
interface S3ObjectStoreOptions {
  /** The S3 bucket name. */
  bucket: string;
  /** Optional existing S3 client instance. */
  client?: S3Client;
  /** Optional configuration for creating a new S3 client. */
  clientConfig?: ConstructorParameters<typeof S3Client>[0];
}

/**
 * S3-backed implementation of the ObjectStore interface.
 * Provides methods for storing, retrieving, streaming, and deleting objects in S3.
 *
 * Encryption is activated per-operation when a `compliance` context is provided.
 * Without it, object bodies are stored and read as plaintext.
 */
export class S3ObjectStore implements ObjectStore<S3Client> {
  private s3: S3Client;
  private bucket: string;
  private initialized: boolean;
  private encryptor?: FieldEncryptor;

  constructor(
    private openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    options: S3ObjectStoreOptions,
    private telemetryOptions: TelemetryOptions,
    encryption: S3EncryptionOptions
  ) {
    this.s3 = options.client || new S3Client(options.clientConfig || {});
    this.bucket = options.bucket;
    this.initialized = false;
    this.encryptor = encryption.encryptor;
  }

  // ---------------------------------------------------------------------------
  // Encryption helpers — only active when compliance context is provided
  // ---------------------------------------------------------------------------

  private encryptBody(body: string, compliance?: ComplianceContext): string {
    if (!compliance || !this.encryptor) return body;
    return this.encryptor.encrypt(body, compliance.tenantId) ?? body;
  }

  private decryptBody(body: string, compliance?: ComplianceContext): string {
    if (!compliance || !this.encryptor) return body;
    if (!isEncrypted(body)) return body;
    try {
      return this.encryptor.decrypt(body, compliance.tenantId) ?? body;
    } catch {
      return body;
    }
  }

  // ---------------------------------------------------------------------------
  // Internal
  // ---------------------------------------------------------------------------

  private async ensureBucketExists() {
    try {
      await this.s3.send(new HeadBucketCommand({ Bucket: this.bucket }));
    } catch {
      await this.s3.send(new CreateBucketCommand({ Bucket: this.bucket }));
    }

    this.initialized = true;
  }

  async putObject<T>(
    object: T & { key: string },
    compliance?: ComplianceContext
  ): Promise<void> {
    if (!this.initialized) {
      await this.ensureBucketExists();
    }

    const { key, ...rest } = object;
    const body = this.encryptBody(JSON.stringify(rest), compliance);
    const params: PutObjectCommandInput = {
      Bucket: this.bucket,
      Key: key,
      Body: body,
      ContentType: 'application/json'
    };
    await this.s3.send(new PutObjectCommand(params));
  }

  async putBatchObjects<T>(
    objects: (T & { key: string })[],
    compliance?: ComplianceContext
  ): Promise<void> {
    await Promise.all(objects.map((obj) => this.putObject(obj, compliance)));
  }

  async streamUploadObject<T>(
    object: T & { key: string },
    compliance?: ComplianceContext
  ): Promise<void> {
    await this.putObject(object, compliance);
  }

  async streamUploadBatchObjects<T>(
    objects: (T & { key: string })[],
    compliance?: ComplianceContext
  ): Promise<void> {
    await this.putBatchObjects(objects, compliance);
  }

  async deleteObject(objectKey: string): Promise<void> {
    await this.s3.send(
      new DeleteObjectCommand({ Bucket: this.bucket, Key: objectKey })
    );
  }

  async deleteBatchObjects(objectKeys: string[]): Promise<void> {
    const params: DeleteObjectsCommandInput = {
      Bucket: this.bucket,
      Delete: {
        Objects: objectKeys.map((Key) => ({ Key }))
      }
    };
    await this.s3.send(new DeleteObjectsCommand(params));
  }

  async readObject<T>(
    objectKey: string,
    compliance?: ComplianceContext
  ): Promise<T> {
    const resp = await this.s3.send(
      new GetObjectCommand({ Bucket: this.bucket, Key: objectKey })
    );

    if (!resp.Body) {
      throw new Error('S3 did not return a body');
    }

    const raw = await resp.Body.transformToString();
    return JSON.parse(this.decryptBody(raw, compliance)) as T;
  }

  async readBatchObjects<T>(
    objectKeys: string[],
    compliance?: ComplianceContext
  ): Promise<T[]> {
    return Promise.all(
      objectKeys.map((key) => this.readObject<T>(key, compliance))
    );
  }

  async streamDownloadObject(objectKey: string): Promise<Readable> {
    const resp = await this.s3.send(
      new GetObjectCommand({ Bucket: this.bucket, Key: objectKey })
    );
    const webStream = resp.Body?.transformToWebStream();
    if (!webStream) {
      throw new Error('S3 did not return a stream');
    }

    return Readable.fromWeb(
      webStream as Parameters<typeof Readable.fromWeb>[0]
    );
  }

  async streamDownloadBatchObjects(objectKeys: string[]): Promise<Readable[]> {
    return Promise.all(objectKeys.map((key) => this.streamDownloadObject(key)));
  }

  getClient(): S3Client {
    return this.s3;
  }
}

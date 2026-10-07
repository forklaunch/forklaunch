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
import {
  ObjectStore,
  type ObjectStoreFileBody,
  type PresignDownloadOptions,
  type PresignedUpload,
  type PresignUploadOptions,
  type PutFileOptions
} from '@forklaunch/core/objectstore';
import type { ComplianceContext } from '@forklaunch/core/cache';
import { Upload } from '@aws-sdk/lib-storage';
import {
  createPresignedPost,
  type PresignedPostOptions
} from '@aws-sdk/s3-presigned-post';
import { getSignedUrl } from '@aws-sdk/s3-request-presigner';
import { Readable } from 'stream';

const ENCRYPTED_PREFIXES = ['v1:', 'v2:', 'v3:'] as const;

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
  /**
   * Key prefix every operation is confined to (`S3_PREFIX`), for a bucket
   * shared between tenants or instances. Keys passed to the store are relative
   * to it.
   */
  prefix?: string;
  /**
   * Create the bucket on first write when it is missing. Default: only when a
   * custom endpoint is configured (local MinIO). Deployed buckets are
   * provisioned by the platform, and their task roles may not create buckets.
   */
  createBucketIfMissing?: boolean;
  /**
   * Upper bounds on presigned link lifetimes, in seconds, set by the platform
   * (`S3_PRESIGN_MAX_UPLOAD_SECONDS`, `S3_PRESIGN_MAX_DOWNLOAD_SECONDS`).
   * Defaults: 900 for uploads, 300 for downloads.
   */
  presignLimits?: { maxUploadSeconds?: number; maxDownloadSeconds?: number };
}

/** A file body: bytes, text or a stream (streams upload in parts). */
export type S3FileBody = ObjectStoreFileBody;
export type {
  PresignDownloadOptions,
  PresignedUpload,
  PresignUploadOptions,
  PutFileOptions
};

const DEFAULT_MAX_UPLOAD_SECONDS = 900;
const DEFAULT_MAX_DOWNLOAD_SECONDS = 300;

/**
 * S3 client configuration from the usual environment variables.
 *
 * Deployed on ForkLaunch, only the region is set: credentials come from the
 * service's task role through the default AWS credential chain, so no key is
 * stored in the app. Static keys are passed only when both are present (local
 * MinIO, or a bucket outside ForkLaunch). A custom endpoint implies path-style
 * addressing, which MinIO needs.
 */
export function s3ClientConfig(env: {
  url?: string;
  region?: string;
  accessKeyId?: string;
  secretAccessKey?: string;
}): NonNullable<ConstructorParameters<typeof S3Client>[0]> {
  return {
    ...(env.region ? { region: env.region } : {}),
    ...(env.url ? { endpoint: env.url, forcePathStyle: true } : {}),
    ...(env.accessKeyId && env.secretAccessKey
      ? {
          credentials: {
            accessKeyId: env.accessKeyId,
            secretAccessKey: env.secretAccessKey
          }
        }
      : {})
  };
}

/** A lifetime cap from the environment; a missing or invalid value is the default. */
function positiveSeconds(value: number | undefined, fallback: number): number {
  return value !== undefined && Number.isFinite(value) && value > 0
    ? Math.floor(value)
    : fallback;
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
  private prefix: string;
  private createBucketIfMissing: boolean;
  private maxUploadSeconds: number;
  private maxDownloadSeconds: number;

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
    this.prefix = normalizePrefix(options.prefix);
    this.createBucketIfMissing =
      options.createBucketIfMissing ?? Boolean(options.clientConfig?.endpoint);
    this.maxUploadSeconds = positiveSeconds(
      options.presignLimits?.maxUploadSeconds,
      DEFAULT_MAX_UPLOAD_SECONDS
    );
    this.maxDownloadSeconds = positiveSeconds(
      options.presignLimits?.maxDownloadSeconds,
      DEFAULT_MAX_DOWNLOAD_SECONDS
    );
  }

  /** The full object key for a key relative to this store. */
  private key(objectKey: string): string {
    let start = 0;
    while (objectKey[start] === '/') start += 1;
    return `${this.prefix}${objectKey.slice(start)}`;
  }

  private lifetime(
    requested: number | undefined,
    cap: number,
    kind: 'upload' | 'download'
  ): number {
    if (requested === undefined) return cap;
    if (!Number.isFinite(requested) || requested <= 0) {
      throw new Error(`Presigned ${kind} lifetime must be a positive number`);
    }
    if (requested > cap) {
      throw new Error(
        `Presigned ${kind} lifetime of ${requested}s exceeds the cap of ${cap}s ` +
          `(S3_PRESIGN_MAX_${kind.toUpperCase()}_SECONDS)`
      );
    }
    return Math.floor(requested);
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
    if (!this.createBucketIfMissing) {
      this.initialized = true;
      return;
    }
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
      Key: this.key(key),
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
      new DeleteObjectCommand({ Bucket: this.bucket, Key: this.key(objectKey) })
    );
  }

  async deleteBatchObjects(objectKeys: string[]): Promise<void> {
    const params: DeleteObjectsCommandInput = {
      Bucket: this.bucket,
      Delete: {
        Objects: objectKeys.map((Key) => ({ Key: this.key(Key) }))
      }
    };
    await this.s3.send(new DeleteObjectsCommand(params));
  }

  async readObject<T>(
    objectKey: string,
    compliance?: ComplianceContext
  ): Promise<T> {
    const resp = await this.s3.send(
      new GetObjectCommand({ Bucket: this.bucket, Key: this.key(objectKey) })
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
      new GetObjectCommand({ Bucket: this.bucket, Key: this.key(objectKey) })
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

  /**
   * Store a file as-is: bytes, text or a stream, with its content type.
   * Unlike `putObject`, the body is not JSON-encoded and not encrypted by the
   * app; the bucket's own KMS encryption applies. Streams upload in parts.
   */
  async putFile(
    objectKey: string,
    body: S3FileBody,
    options: PutFileOptions
  ): Promise<void> {
    if (!this.initialized) {
      await this.ensureBucketExists();
    }
    const params: PutObjectCommandInput = {
      Bucket: this.bucket,
      Key: this.key(objectKey),
      Body: body,
      ContentType: options.contentType,
      ...(options.filename
        ? { ContentDisposition: contentDisposition(options.filename) }
        : {}),
      ...(options.metadata ? { Metadata: options.metadata } : {})
    };
    if (body instanceof Readable) {
      await new Upload({ client: this.s3, params }).done();
      return;
    }
    await this.s3.send(new PutObjectCommand(params));
  }

  /**
   * A short-lived grant for a browser to upload one file straight to S3.
   * It is a presigned POST, so S3 itself enforces the size limit and the
   * content type. The bucket's CORS rules, set by the platform, decide which
   * sites may use it.
   */
  async presignUpload(
    objectKey: string,
    options: PresignUploadOptions
  ): Promise<PresignedUpload> {
    if (!Number.isInteger(options.maxBytes) || options.maxBytes <= 0) {
      throw new Error('presignUpload needs a positive integer maxBytes');
    }
    if (!this.initialized) {
      await this.ensureBucketExists();
    }
    const expires = this.lifetime(
      options.expiresIn,
      this.maxUploadSeconds,
      'upload'
    );
    const key = this.key(objectKey);
    const post: PresignedPostOptions = {
      Bucket: this.bucket,
      Key: key,
      Conditions: [
        ['content-length-range', 1, options.maxBytes],
        ['eq', '$Content-Type', options.contentType]
      ],
      Fields: { 'Content-Type': options.contentType },
      Expires: expires
    };
    const { url, fields } = await createPresignedPost(this.s3, post);
    return {
      url,
      fields,
      key: objectKey,
      expiresAt: new Date(Date.now() + expires * 1000)
    };
  }

  /** A short-lived link to download one object. */
  async presignDownload(
    objectKey: string,
    options: PresignDownloadOptions = {}
  ): Promise<string> {
    const expiresIn = this.lifetime(
      options.expiresIn,
      this.maxDownloadSeconds,
      'download'
    );
    return getSignedUrl(
      this.s3,
      new GetObjectCommand({
        Bucket: this.bucket,
        Key: this.key(objectKey),
        ...(options.filename
          ? { ResponseContentDisposition: contentDisposition(options.filename) }
          : {})
      }),
      { expiresIn }
    );
  }

  getClient(): S3Client {
    return this.s3;
  }
}

/** `a/b` and `a/b/` both confine keys under `a/b/`; empty means no prefix. */
function normalizePrefix(prefix: string | undefined): string {
  if (!prefix) return '';
  let start = 0;
  while (prefix[start] === '/') start += 1;
  let end = prefix.length;
  while (end > start && prefix[end - 1] === '/') end -= 1;
  return end > start ? `${prefix.slice(start, end)}/` : '';
}

/** An attachment Content-Disposition with an RFC 5987 UTF-8 filename. */
function contentDisposition(filename: string): string {
  const ascii = filename.replace(/[^\x20-\x7e]|["\\]/g, '_');
  return `attachment; filename="${ascii}"; filename*=UTF-8''${encodeURIComponent(filename)}`;
}

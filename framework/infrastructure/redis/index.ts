import { safeParse, safeStringify } from '@forklaunch/common';
import {
  type ComplianceContext,
  TtlCache,
  TtlCacheRecord
} from '@forklaunch/core/cache';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { type FieldEncryptor } from '@forklaunch/core/persistence';
import { createClient, RedisClientOptions } from 'redis';

/**
 * Type representing a raw reply from Redis commands.
 * Can be a string, number, Buffer, null, undefined, or array of raw replies.
 */
type RedisCommandRawReply =
  | string
  | number
  | Buffer
  | null
  | undefined
  | Array<RedisCommandRawReply>;

const ENCRYPTED_PREFIXES = ['v1:', 'v2:'] as const;

function isEncrypted(value: string): boolean {
  return ENCRYPTED_PREFIXES.some((p) => value.startsWith(p));
}

/**
 * Options for configuring encryption on the Redis cache.
 * Required — every consumer must explicitly configure encryption.
 */
export interface RedisCacheEncryptionOptions {
  /** The FieldEncryptor instance to use for encrypting cache values. */
  encryptor: FieldEncryptor;
}

/**
 * Class representing a Redis-based TTL (Time-To-Live) cache.
 * Implements the TtlCache interface to provide caching functionality with automatic expiration.
 *
 * Encryption is activated per-operation when a `compliance` context is provided.
 * Without it, values are stored and read as plaintext.
 */
export class RedisTtlCache implements TtlCache {
  private client;
  private telemetryOptions;
  private encryptor?: FieldEncryptor;

  /**
   * Creates an instance of RedisTtlCache.
   *
   * @param {number} ttlMilliseconds - The default Time-To-Live in milliseconds for cache entries
   * @param {OpenTelemetryCollector<MetricsDefinition>} openTelemetryCollector - Collector for OpenTelemetry metrics
   * @param {RedisClientOptions} options - Configuration options for the Redis client
   * @param {TelemetryOptions} telemetryOptions - Configuration options for telemetry
   * @param {RedisCacheEncryptionOptions} encryption - Encryption configuration
   */
  constructor(
    private ttlMilliseconds: number,
    private openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    options: RedisClientOptions,
    telemetryOptions: TelemetryOptions,
    encryption: RedisCacheEncryptionOptions
  ) {
    this.telemetryOptions = evaluateTelemetryOptions(telemetryOptions);
    this.client = createClient(options);
    this.encryptor = encryption.encryptor;
    if (this.telemetryOptions.enabled.logging) {
      this.client.on('error', (err) => this.openTelemetryCollector.error(err));
      this.client.connect().catch(this.openTelemetryCollector.error);
    }
  }

  // ---------------------------------------------------------------------------
  // Encryption helpers — only active when compliance context is provided
  // ---------------------------------------------------------------------------

  private encryptValue(
    serialized: string,
    compliance?: ComplianceContext
  ): string {
    if (!compliance || !this.encryptor) return serialized;
    return (
      this.encryptor.encrypt(serialized, compliance.tenantId) ?? serialized
    );
  }

  private decryptValue(value: string, compliance?: ComplianceContext): string {
    if (!compliance || !this.encryptor) return value;
    if (!isEncrypted(value)) return value;
    try {
      return this.encryptor.decrypt(value, compliance.tenantId) ?? value;
    } catch {
      return value;
    }
  }

  private parseValue<T>(
    value: RedisCommandRawReply,
    compliance?: ComplianceContext
  ): T {
    if (value == null) {
      return null as T;
    }

    if (Array.isArray(value)) {
      return value.map((v) => this.parseValue<T>(v, compliance)) as T;
    }

    if (Buffer.isBuffer(value)) {
      return value.toJSON() as T;
    }

    switch (typeof value) {
      case 'object':
      case 'string':
        return safeParse(this.decryptValue(String(value), compliance)) as T;
      case 'number':
        return value as T;
    }
  }

  // ---------------------------------------------------------------------------
  // TtlCache implementation
  // ---------------------------------------------------------------------------

  async putRecord<T>(
    { key, value, ttlMilliseconds = this.ttlMilliseconds }: TtlCacheRecord<T>,
    compliance?: ComplianceContext
  ): Promise<void> {
    if (this.telemetryOptions.enabled.logging) {
      this.openTelemetryCollector.info(`Putting record into cache: ${key}`);
    }
    await this.client.set(
      key,
      this.encryptValue(safeStringify(value), compliance),
      { PX: ttlMilliseconds }
    );
  }

  async putBatchRecords<T>(
    cacheRecords: TtlCacheRecord<T>[],
    compliance?: ComplianceContext
  ): Promise<void> {
    const multiCommand = this.client.multi();
    for (const { key, value, ttlMilliseconds } of cacheRecords) {
      multiCommand.set(
        key,
        this.encryptValue(safeStringify(value), compliance),
        { PX: ttlMilliseconds || this.ttlMilliseconds }
      );
    }
    await multiCommand.exec();
  }

  async enqueueRecord<T>(
    queueName: string,
    value: T,
    compliance?: ComplianceContext
  ): Promise<void> {
    await this.client.lPush(
      queueName,
      this.encryptValue(safeStringify(value), compliance)
    );
  }

  async enqueueBatchRecords<T>(
    queueName: string,
    values: T[],
    compliance?: ComplianceContext
  ): Promise<void> {
    const multiCommand = this.client.multi();
    for (const value of values) {
      multiCommand.lPush(
        queueName,
        this.encryptValue(safeStringify(value), compliance)
      );
    }
    await multiCommand.exec();
  }

  async deleteRecord(cacheRecordKey: string): Promise<void> {
    await this.client.del(cacheRecordKey);
  }

  async deleteBatchRecords(cacheRecordKeys: string[]): Promise<void> {
    const multiCommand = this.client.multi();
    for (const key of cacheRecordKeys) {
      multiCommand.del(key);
    }
    await multiCommand.exec();
  }

  async dequeueRecord<T>(
    queueName: string,
    compliance?: ComplianceContext
  ): Promise<T> {
    const value = await this.client.rPop(queueName);
    if (value === null) {
      throw new Error(`Queue is empty: ${queueName}`);
    }
    return safeParse(this.decryptValue(value, compliance)) as T;
  }

  async dequeueBatchRecords<T>(
    queueName: string,
    pageSize: number,
    compliance?: ComplianceContext
  ): Promise<T[]> {
    const multiCommand = this.client.multi();
    for (let i = 0; i < pageSize; i++) {
      multiCommand.rPop(queueName);
    }
    const values = await multiCommand.exec();
    return values
      .map((value) =>
        this.parseValue<T>(value as unknown as RedisCommandRawReply, compliance)
      )
      .filter(Boolean);
  }

  async readRecord<T>(
    cacheRecordKey: string,
    compliance?: ComplianceContext
  ): Promise<TtlCacheRecord<T>> {
    const [value, ttl] = await this.client
      .multi()
      .get(cacheRecordKey)
      .ttl(cacheRecordKey)
      .exec();
    if (value === null) {
      throw new Error(`Record not found for key: ${cacheRecordKey}`);
    }

    return {
      key: cacheRecordKey,
      value: this.parseValue<T>(
        value as unknown as RedisCommandRawReply,
        compliance
      ),
      ttlMilliseconds:
        this.parseValue<number>(
          ttl as unknown as RedisCommandRawReply,
          compliance
        ) * 1000
    };
  }

  async readBatchRecords<T>(
    cacheRecordKeysOrPrefix: string[] | string,
    compliance?: ComplianceContext
  ): Promise<TtlCacheRecord<T>[]> {
    const keys = Array.isArray(cacheRecordKeysOrPrefix)
      ? cacheRecordKeysOrPrefix
      : await this.client.keys(cacheRecordKeysOrPrefix + '*');
    const multiCommand = this.client.multi();
    for (const key of keys) {
      multiCommand.get(key);
      multiCommand.ttl(key);
    }
    const values = await multiCommand.exec();
    return values.reduce<TtlCacheRecord<T>[]>((acc, value, index) => {
      if (index % 2 === 0) {
        const maybeValue = this.parseValue<T>(
          value as unknown as RedisCommandRawReply,
          compliance
        );
        const ttl = this.parseValue<number>(
          values[index + 1] as unknown as RedisCommandRawReply,
          compliance
        );
        if (maybeValue && ttl) {
          acc.push({
            key: keys[index / 2],
            value: maybeValue,
            ttlMilliseconds: ttl * 1000
          });
        }
      }
      return acc;
    }, []);
  }

  async listKeys(pattern_prefix: string): Promise<string[]> {
    return this.client.keys(pattern_prefix + '*');
  }

  async peekRecord(cacheRecordKey: string): Promise<boolean> {
    const result = await this.client.exists(cacheRecordKey);
    return result === 1;
  }

  async peekBatchRecords(
    cacheRecordKeysOrPrefix: string[] | string
  ): Promise<boolean[]> {
    const keys = Array.isArray(cacheRecordKeysOrPrefix)
      ? cacheRecordKeysOrPrefix
      : await this.client.keys(cacheRecordKeysOrPrefix + '*');
    const multiCommand = this.client.multi();
    for (const key of keys) {
      multiCommand.exists(key);
    }
    const results = await multiCommand.exec();
    return results.map((result) => (result as unknown as number) === 1);
  }

  async peekQueueRecord<T>(
    queueName: string,
    compliance?: ComplianceContext
  ): Promise<T> {
    const value = await this.client.lRange(queueName, 0, 0);
    return this.parseValue<T>(value[0], compliance);
  }

  async peekQueueRecords<T>(
    queueName: string,
    pageSize: number,
    compliance?: ComplianceContext
  ): Promise<T[]> {
    const values = await this.client.lRange(queueName, 0, pageSize - 1);
    return values
      .map((value) => this.parseValue<T>(value, compliance))
      .filter(Boolean);
  }

  async disconnect(): Promise<void> {
    await this.client.quit();
  }

  getTtlMilliseconds(): number {
    return this.ttlMilliseconds;
  }

  getClient(): typeof this.client {
    return this.client;
  }
}

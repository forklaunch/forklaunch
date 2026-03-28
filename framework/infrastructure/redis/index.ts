import { safeParse, safeStringify } from '@forklaunch/common';
import { TtlCache, TtlCacheRecord } from '@forklaunch/core/cache';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import {
  getCurrentTenantId,
  type FieldEncryptor
} from '@forklaunch/core/persistence';
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
  /** Set to true to disable encryption. Defaults to false (encryption enabled). */
  disabled?: boolean;
}

/**
 * Class representing a Redis-based TTL (Time-To-Live) cache.
 * Implements the TtlCache interface to provide caching functionality with automatic expiration.
 *
 * Encryption is enabled by default when an encryptor is provided. Values are encrypted
 * before storage and decrypted on read using AES-256-GCM with per-tenant key derivation.
 */
export class RedisTtlCache implements TtlCache {
  private client;
  private telemetryOptions;
  private encryptor?: FieldEncryptor;
  private encryptionDisabled: boolean;

  /**
   * Creates an instance of RedisTtlCache.
   *
   * @param {number} ttlMilliseconds - The default Time-To-Live in milliseconds for cache entries
   * @param {OpenTelemetryCollector<MetricsDefinition>} openTelemetryCollector - Collector for OpenTelemetry metrics
   * @param {RedisClientOptions} options - Configuration options for the Redis client
   * @param {TelemetryOptions} telemetryOptions - Configuration options for telemetry
   * @param {RedisCacheEncryptionOptions} encryption - Encryption configuration (enabled by default)
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
    this.encryptionDisabled = encryption.disabled ?? false;
    if (this.telemetryOptions.enabled.logging) {
      this.client.on('error', (err) => this.openTelemetryCollector.error(err));
      this.client.connect().catch(this.openTelemetryCollector.error);
    }
  }

  // ---------------------------------------------------------------------------
  // Encryption helpers
  // ---------------------------------------------------------------------------

  private encryptValue(serialized: string): string {
    if (!this.encryptor || this.encryptionDisabled) return serialized;
    return (
      this.encryptor.encrypt(serialized, getCurrentTenantId()) ?? serialized
    );
  }

  private decryptValue(value: string): string {
    if (!this.encryptor || this.encryptionDisabled) return value;
    if (!isEncrypted(value)) return value;
    try {
      return this.encryptor.decrypt(value, getCurrentTenantId()) ?? value;
    } catch {
      return value;
    }
  }

  /**
   * Parses a raw Redis reply into the expected type.
   * Handles null values, arrays, buffers, and JSON strings.
   *
   * @template T - The expected type of the parsed value
   * @param {RedisCommandRawReply} value - The raw value from Redis to parse
   * @returns {T} The parsed value cast to type T
   */
  private parseValue<T>(value: RedisCommandRawReply): T {
    if (value == null) {
      return null as T;
    }

    if (Array.isArray(value)) {
      return value.map((v) => this.parseValue<T>(v)) as T;
    }

    if (Buffer.isBuffer(value)) {
      return value.toJSON() as T;
    }

    switch (typeof value) {
      case 'object':
      case 'string':
        return safeParse(this.decryptValue(String(value))) as T;
      case 'number':
        return value as T;
    }
  }

  /**
   * Puts a record into the Redis cache.
   *
   * @template T - The type of value being cached
   * @param {TtlCacheRecord<T>} param0 - The cache record containing key, value and optional TTL
   * @param {string} param0.key - The key to store the value under
   * @param {T} param0.value - The value to cache
   * @param {number} [param0.ttlMilliseconds] - Optional TTL in milliseconds, defaults to constructor value
   * @returns {Promise<void>} A promise that resolves when the value is cached
   */
  async putRecord<T>({
    key,
    value,
    ttlMilliseconds = this.ttlMilliseconds
  }: TtlCacheRecord<T>): Promise<void> {
    if (this.telemetryOptions.enabled.logging) {
      this.openTelemetryCollector.info(`Putting record into cache: ${key}`);
    }
    await this.client.set(key, this.encryptValue(safeStringify(value)), {
      PX: ttlMilliseconds
    });
  }

  /**
   * Puts multiple records into the Redis cache in a single transaction.
   *
   * @template T - The type of values being cached
   * @param {TtlCacheRecord<T>[]} cacheRecords - Array of cache records to store
   * @returns {Promise<void>} A promise that resolves when all values are cached
   */
  async putBatchRecords<T>(cacheRecords: TtlCacheRecord<T>[]): Promise<void> {
    const multiCommand = this.client.multi();
    for (const { key, value, ttlMilliseconds } of cacheRecords) {
      multiCommand.set(key, this.encryptValue(safeStringify(value)), {
        PX: ttlMilliseconds || this.ttlMilliseconds
      });
    }
    await multiCommand.exec();
  }

  /**
   * Adds a value to the left end of a Redis list.
   *
   * @template T - The type of value being enqueued
   * @param {string} queueName - The name of the Redis list
   * @param {T} value - The value to add to the list
   * @returns {Promise<void>} A promise that resolves when the value is enqueued
   */
  async enqueueRecord<T>(queueName: string, value: T): Promise<void> {
    await this.client.lPush(queueName, this.encryptValue(safeStringify(value)));
  }

  /**
   * Adds multiple values to the left end of a Redis list in a single transaction.
   *
   * @template T - The type of values being enqueued
   * @param {string} queueName - The name of the Redis list
   * @param {T[]} values - Array of values to add to the list
   * @returns {Promise<void>} A promise that resolves when all values are enqueued
   */
  async enqueueBatchRecords<T>(queueName: string, values: T[]): Promise<void> {
    const multiCommand = this.client.multi();
    for (const value of values) {
      multiCommand.lPush(queueName, this.encryptValue(safeStringify(value)));
    }
    await multiCommand.exec();
  }

  /**
   * Deletes a record from the Redis cache.
   *
   * @param {string} cacheRecordKey - The key of the record to delete
   * @returns {Promise<void>} A promise that resolves when the record is deleted
   */
  async deleteRecord(cacheRecordKey: string): Promise<void> {
    await this.client.del(cacheRecordKey);
  }

  /**
   * Deletes multiple records from the Redis cache in a single transaction.
   *
   * @param {string[]} cacheRecordKeys - Array of keys to delete
   * @returns {Promise<void>} A promise that resolves when all records are deleted
   */
  async deleteBatchRecords(cacheRecordKeys: string[]): Promise<void> {
    const multiCommand = this.client.multi();
    for (const key of cacheRecordKeys) {
      multiCommand.del(key);
    }
    await multiCommand.exec();
  }

  /**
   * Removes and returns the rightmost element from a Redis list.
   *
   * @template T - The type of value being dequeued
   * @param {string} queueName - The name of the Redis list
   * @returns {Promise<T>} A promise that resolves with the dequeued value
   * @throws {Error} If the queue is empty
   */
  async dequeueRecord<T>(queueName: string): Promise<T> {
    const value = await this.client.rPop(queueName);
    if (value === null) {
      throw new Error(`Queue is empty: ${queueName}`);
    }
    return safeParse(this.decryptValue(value)) as T;
  }

  /**
   * Removes and returns multiple elements from the right end of a Redis list.
   *
   * @template T - The type of values being dequeued
   * @param {string} queueName - The name of the Redis list
   * @param {number} pageSize - Maximum number of elements to dequeue
   * @returns {Promise<T[]>} A promise that resolves with an array of dequeued values
   */
  async dequeueBatchRecords<T>(
    queueName: string,
    pageSize: number
  ): Promise<T[]> {
    const multiCommand = this.client.multi();
    for (let i = 0; i < pageSize; i++) {
      multiCommand.rPop(queueName);
    }
    const values = await multiCommand.exec();
    return values
      .map((value) =>
        this.parseValue<T>(value as unknown as RedisCommandRawReply)
      )
      .filter(Boolean);
  }

  /**
   * Reads a record from the Redis cache.
   *
   * @template T - The type of value being read
   * @param {string} cacheRecordKey - The key of the record to read
   * @returns {Promise<TtlCacheRecord<T>>} A promise that resolves with the cache record
   * @throws {Error} If the record is not found
   */
  async readRecord<T>(cacheRecordKey: string): Promise<TtlCacheRecord<T>> {
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
      value: this.parseValue<T>(value as unknown as RedisCommandRawReply),
      ttlMilliseconds:
        this.parseValue<number>(ttl as unknown as RedisCommandRawReply) * 1000
    };
  }

  /**
   * Reads multiple records from the Redis cache.
   *
   * @template T - The type of values being read
   * @param {string[] | string} cacheRecordKeysOrPrefix - Array of keys to read, or a prefix pattern
   * @returns {Promise<TtlCacheRecord<T>[]>} A promise that resolves with an array of cache records
   */
  async readBatchRecords<T>(
    cacheRecordKeysOrPrefix: string[] | string
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
          value as unknown as RedisCommandRawReply
        );
        const ttl = this.parseValue<number>(
          values[index + 1] as unknown as RedisCommandRawReply
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

  /**
   * Lists all keys in the Redis cache that match a pattern prefix.
   *
   * @param {string} pattern_prefix - The prefix pattern to match keys against
   * @returns {Promise<string[]>} A promise that resolves with an array of matching keys
   */
  async listKeys(pattern_prefix: string): Promise<string[]> {
    const keys = await this.client.keys(pattern_prefix + '*');
    return keys;
  }

  /**
   * Checks if a record exists in the Redis cache.
   *
   * @param {string} cacheRecordKey - The key to check
   * @returns {Promise<boolean>} A promise that resolves with true if the record exists, false otherwise
   */
  async peekRecord(cacheRecordKey: string): Promise<boolean> {
    const result = await this.client.exists(cacheRecordKey);
    return result === 1;
  }

  /**
   * Checks if multiple records exist in the Redis cache.
   *
   * @param {string[] | string} cacheRecordKeysOrPrefix - Array of keys to check, or a prefix pattern
   * @returns {Promise<boolean[]>} A promise that resolves with an array of existence booleans
   */
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

  /**
   * Peeks at a record in the Redis cache.
   *
   * @template T - The type of value being peeked at
   * @param {string} queueName - The name of the Redis queue
   * @returns {Promise<T>} A promise that resolves with the peeked value
   */
  async peekQueueRecord<T>(queueName: string): Promise<T> {
    const value = await this.client.lRange(queueName, 0, 0);
    return this.parseValue<T>(value[0]);
  }

  /**
   * Peeks at multiple records in the Redis cache.
   *
   * @template T - The type of values being peeked at
   * @param {string} queueName - The name of the Redis queue
   * @param {number} pageSize - The number of records to peek at
   * @returns {Promise<T[]>} A promise that resolves with an array of peeked values
   */
  async peekQueueRecords<T>(queueName: string, pageSize: number): Promise<T[]> {
    const values = await this.client.lRange(queueName, 0, pageSize - 1);
    return values.map((value) => this.parseValue<T>(value)).filter(Boolean);
  }

  /**
   * Gracefully disconnects from the Redis server.
   *
   * @returns {Promise<void>} A promise that resolves when the connection is closed
   */
  async disconnect(): Promise<void> {
    await this.client.quit();
  }

  /**
   * Gets the default Time-To-Live value in milliseconds.
   *
   * @returns {number} The default TTL in milliseconds
   */
  getTtlMilliseconds(): number {
    return this.ttlMilliseconds;
  }

  /**
   * Gets the underlying Redis client instance.
   *
   * @returns {typeof this.client} The Redis client instance
   */
  getClient(): typeof this.client {
    return this.client;
  }
}

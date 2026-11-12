import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { FieldEncryptor } from '@forklaunch/core/persistence';
import { GenericContainer, StartedTestContainer } from 'testcontainers';
import { RedisTtlCache } from '../index';

describe('redisTtlCache', () => {
  let container: StartedTestContainer;
  let cache: RedisTtlCache;
  let key: string;
  let value: unknown;
  let ttlMilliseconds: number;

  beforeAll(async () => {
    container = await new GenericContainer('redis')
      .withExposedPorts(6379)
      .start();

    cache = new RedisTtlCache(
      5000,
      new OpenTelemetryCollector('test'),
      {
        url: `redis://${container.getHost()}:${container.getMappedPort(6379)}`
      },
      {
        enabled: true,
        level: 'info'
      },
      {
        encryptor: new FieldEncryptor('test-encryption-key-for-redis-tests')
      }
    );

    key = 'testKey';
    value = { data: 'testValue' };
    ttlMilliseconds = 1000;
  }, 30000);

  afterAll(async () => {
    if (cache) {
      await cache.disconnect();
    }
    if (container) {
      await container.stop({ remove: true, removeVolumes: true });
    }
  }, 30000);

  it('putRecordIfAbsent writes once, atomically, and expires', async () => {
    const claim = () =>
      cache.putRecordIfAbsent({
        key: 'claim:once',
        value: 1,
        ttlMilliseconds: 300
      });
    // Twenty simultaneous claims of one key: exactly one wins.
    const results = await Promise.all(Array.from({ length: 20 }, claim));
    expect(results.filter(Boolean)).toHaveLength(1);
    expect(await claim()).toBe(false);
    expect(await cache.peekRecord('claim:once')).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(await claim()).toBe(true);
    await cache.deleteRecord('claim:once');
  });

  it('put', async () => {
    await cache.putRecord({ key, value, ttlMilliseconds });
  });

  test('read', async () => {
    const storedValue = await cache.readRecord(key);

    expect(storedValue).toEqual({
      key,
      ttlMilliseconds,
      value
    });
  });

  test('peek', async () => {
    const exists = await cache.peekRecord(key);

    expect(exists).toBeTruthy();
  });

  test('delete', async () => {
    await cache.deleteRecord(key);
    const existsAfterDelete = await cache.peekRecord(key);

    expect(existsAfterDelete).toBeFalsy();
  });

  test('check no record after ttl', async () => {
    await Promise.resolve(setTimeout(async () => {}, ttlMilliseconds));
    const existsAfterTtl = await cache.peekRecord(key);
    expect(existsAfterTtl).toBeFalsy();
  });

  describe('batch operations', () => {
    const batchKeys = ['batchKey1', 'batchKey2', 'batchKey3'];
    const batchValues = [
      { data: 'value1' },
      { data: 'value2' },
      { data: 'value3' }
    ];

    beforeEach(async () => {
      await cache.putBatchRecords(
        batchKeys.map((key, index) => ({
          key,
          value: batchValues[index],
          ttlMilliseconds
        }))
      );
    });

    test('putBatchRecords', async () => {
      const records = await cache.readBatchRecords(batchKeys);
      expect(records).toHaveLength(3);
      expect(records[0].value).toEqual(batchValues[0]);
    });

    test('readBatchRecords', async () => {
      const records = await cache.readBatchRecords(batchKeys);
      expect(records).toHaveLength(3);
      expect(records.map((r) => r.value)).toEqual(batchValues);
    });

    test('deleteBatchRecords', async () => {
      await cache.deleteBatchRecords(batchKeys);
      const exists = await cache.peekBatchRecords(batchKeys);
      expect(exists).toEqual([false, false, false]);
    });

    test('peekBatchRecords', async () => {
      const exists = await cache.peekBatchRecords(batchKeys);
      expect(exists).toEqual([true, true, true]);
    });
  });

  describe('queue operations', () => {
    const queueName = 'testQueue';
    const queueValues = ['value1', 'value2', 'value3'];

    beforeEach(async () => {
      await cache.enqueueBatchRecords(queueName, queueValues);
    });

    test('enqueueRecord and dequeueRecord', async () => {
      await cache.enqueueRecord(queueName, 'newValue');
      const value = await cache.dequeueRecord(queueName);
      expect(value).toBe('value1');
    });

    test('enqueueBatchRecords and dequeueBatchRecords', async () => {
      const values = await cache.dequeueBatchRecords(queueName, 2);
      expect(values).toEqual(['value2', 'value3']);
    });

    test('dequeueRecord throws error on empty queue', async () => {
      const records = await cache.dequeueBatchRecords(queueName, 100);
      expect(records).toEqual([
        'newValue',
        'value1',
        'value2',
        'value3',
        'value1',
        'value2',
        'value3'
      ]);
      await expect(cache.dequeueRecord(queueName)).rejects.toThrow(
        'Queue is empty'
      );
      expect(await cache.dequeueBatchRecords(queueName, 1000)).toEqual([]);
    });
  });

  test('listKeys', async () => {
    await cache.putRecord({
      key: 'prefix_key1',
      value: 'value1',
      ttlMilliseconds
    });
    await cache.putRecord({
      key: 'prefix_key2',
      value: 'value2',
      ttlMilliseconds
    });

    const keys = await cache.listKeys('prefix_');
    expect(keys).toContain('prefix_key1');
    expect(keys).toContain('prefix_key2');
  });

  test('getTtlMilliseconds', () => {
    expect(cache.getTtlMilliseconds()).toBe(5000);
  });

  test('getClient', () => {
    const client = cache.getClient();
    expect(client).toBeDefined();
    expect(typeof client.set).toBe('function');
  });
});

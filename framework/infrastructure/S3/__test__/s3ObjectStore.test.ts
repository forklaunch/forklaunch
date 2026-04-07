import { S3Client } from '@aws-sdk/client-s3';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { FieldEncryptor } from '@forklaunch/core/persistence';
import { Readable } from 'stream';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { S3ObjectStore } from '../index';

// ---------------------------------------------------------------------------
// Mock the AWS SDK so no real network calls are made
// ---------------------------------------------------------------------------
const mockSend = vi.fn();

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build a Web ReadableStream from an array of byte chunks and track reads. */
function makeTrackedWebStream(chunks: Uint8Array[]): {
  webStream: ReadableStream<Uint8Array>;
  getChunksRead: () => number;
} {
  let chunksRead = 0;
  const webStream = new ReadableStream<Uint8Array>({
    pull(controller) {
      if (chunksRead < chunks.length) {
        controller.enqueue(chunks[chunksRead]);
        chunksRead++;
      } else {
        controller.close();
      }
    }
  });
  return { webStream, getChunksRead: () => chunksRead };
}

/** Collect all chunks from a Readable into a single Buffer. */
function collectStream(stream: Readable): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const parts: Uint8Array[] = [];
    stream.on('data', (chunk: Uint8Array) => parts.push(chunk));
    stream.on('end', () => resolve(Buffer.concat(parts)));
    stream.on('error', reject);
  });
}

// ---------------------------------------------------------------------------
// Factory — inject a fake S3Client directly to avoid constructing a real one
// ---------------------------------------------------------------------------

function makeStore(): S3ObjectStore {
  const fakeClient = { send: mockSend } as unknown as S3Client;
  return new S3ObjectStore(
    new OpenTelemetryCollector('test'),
    { bucket: 'test-bucket', client: fakeClient },
    { enabled: false, level: 'info' },
    {
      encryptor: new FieldEncryptor('test-encryption-key-for-s3-tests')
    }
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('S3ObjectStore', () => {
  beforeEach(() => {
    mockSend.mockReset();
  });

  describe('streamDownloadObject', () => {
    it('returns a Node.js Readable stream', async () => {
      const chunk = new Uint8Array(Buffer.from('hello'));
      const { webStream } = makeTrackedWebStream([chunk]);

      mockSend.mockResolvedValue({
        Body: { transformToWebStream: () => webStream }
      });

      const store = makeStore();
      const result = await store.streamDownloadObject('some-key');

      expect(result).toBeInstanceOf(Readable);
    });

    it('streams all data correctly to the consumer', async () => {
      const chunks = [
        new Uint8Array(Buffer.from('chunk-1')),
        new Uint8Array(Buffer.from('chunk-2')),
        new Uint8Array(Buffer.from('chunk-3'))
      ];
      const { webStream } = makeTrackedWebStream(chunks);

      mockSend.mockResolvedValue({
        Body: { transformToWebStream: () => webStream }
      });

      const store = makeStore();
      const stream = await store.streamDownloadObject('some-key');
      const result = await collectStream(stream);

      expect(result).toEqual(Buffer.concat(chunks));
    });

    it('throws when S3 does not return a stream body', async () => {
      mockSend.mockResolvedValue({ Body: undefined });

      const store = makeStore();

      await expect(store.streamDownloadObject('missing-key')).rejects.toThrow(
        'S3 did not return a stream'
      );
    });

    it('throws when transformToWebStream() returns undefined', async () => {
      mockSend.mockResolvedValue({
        Body: { transformToWebStream: () => undefined }
      });

      const store = makeStore();

      await expect(store.streamDownloadObject('missing-key')).rejects.toThrow(
        'S3 did not return a stream'
      );
    });

    /**
     * Backpressure regression test — mirrors the failing test described in the
     * bug report.
     *
     * With the OLD while(true) loop implementation every chunk was pulled from
     * the Web Stream inside a single _read() call, ignoring backpressure. All
     * data was buffered in the Node.js stream internals immediately, and there
     * was no way to observe how many consumer-side 'data' events fired.
     *
     * With the fix (Readable.fromWeb) Node.js correctly only delivers data to
     * event listeners when the consumer is ready. When we pause after the first
     * chunk, the consumer should have received exactly 1 chunk regardless of
     * how many chunks the stream may have internally pre-fetched.
     */
    it('does NOT deliver more chunks to the consumer after it pauses', async () => {
      const numChunks = 20;
      const chunks = Array.from(
        { length: numChunks },
        (_, i) => new Uint8Array(Buffer.from(`chunk-${i}`))
      );
      const { webStream } = makeTrackedWebStream(chunks);

      mockSend.mockResolvedValue({
        Body: { transformToWebStream: () => webStream }
      });

      const store = makeStore();
      const nodeStream = await store.streamDownloadObject('large-key');

      let chunksReceivedByConsumer = 0;

      // Count every chunk delivered to the consumer
      nodeStream.on('data', () => {
        chunksReceivedByConsumer++;
        // Pause immediately after the first chunk
        if (chunksReceivedByConsumer === 1) {
          nodeStream.pause();
        }
      });

      // Resume the stream (starts flowing)
      nodeStream.resume();

      // Give the event loop a few ticks so any eager pushes can settle
      await new Promise((r) => setTimeout(r, 50));

      // The consumer must have paused after receiving exactly 1 chunk
      expect(chunksReceivedByConsumer).toBe(1);
    });
  });

  describe('streamDownloadBatchObjects', () => {
    it('returns an array of Readable streams, one per key', async () => {
      const makeBody = (text: string) => ({
        transformToWebStream: () =>
          makeTrackedWebStream([new Uint8Array(Buffer.from(text))]).webStream
      });

      mockSend
        .mockResolvedValueOnce({ Body: makeBody('data-1') })
        .mockResolvedValueOnce({ Body: makeBody('data-2') })
        .mockResolvedValueOnce({ Body: makeBody('data-3') });

      const store = makeStore();
      const streams = await store.streamDownloadBatchObjects([
        'key-1',
        'key-2',
        'key-3'
      ]);

      expect(streams).toHaveLength(3);
      streams.forEach((s) => expect(s).toBeInstanceOf(Readable));
    });

    it('streams correct data for each key', async () => {
      const payloads = ['payload-A', 'payload-B'];

      for (const p of payloads) {
        mockSend.mockResolvedValueOnce({
          Body: {
            transformToWebStream: () =>
              makeTrackedWebStream([new Uint8Array(Buffer.from(p))]).webStream
          }
        });
      }

      const store = makeStore();
      const streams = await store.streamDownloadBatchObjects(['k1', 'k2']);

      const [buf1, buf2] = await Promise.all(streams.map(collectStream));

      expect(buf1.toString()).toBe('payload-A');
      expect(buf2.toString()).toBe('payload-B');
    });
  });
});

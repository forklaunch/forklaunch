import { S3Client } from '@aws-sdk/client-s3';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { FieldEncryptor } from '@forklaunch/core/persistence';
import { Readable } from 'stream';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { S3ObjectStore, s3ClientConfig } from '../index';

const mockSend = vi.fn();

function store(
  options: Partial<ConstructorParameters<typeof S3ObjectStore>[1]> = {}
) {
  return new S3ObjectStore(
    new OpenTelemetryCollector('test'),
    { bucket: 'files', ...options },
    { enabled: false, level: 'info' },
    { encryptor: new FieldEncryptor('test-encryption-key-for-s3-tests') }
  );
}

const fake = () =>
  ({ send: mockSend, config: { requestHandler: {} } }) as unknown as S3Client;

/** A real client with static credentials: presigning is local, no network. */
const signingClient = () =>
  new S3Client({
    region: 'us-west-2',
    credentials: { accessKeyId: 'AKIDEXAMPLE', secretAccessKey: 'secret' }
  });

describe('S3ObjectStore files and presigned links', () => {
  beforeEach(() => mockSend.mockReset().mockResolvedValue({}));

  it('confines every key to the prefix', async () => {
    const s = store({ client: fake(), prefix: '/inst-42/' });
    await s.putFile('/forms/a.pdf', Buffer.from('x'), {
      contentType: 'application/pdf'
    });
    await s.deleteObject('forms/a.pdf');
    const keys = mockSend.mock.calls.map((c) => c[0].input.Key);
    expect(keys).toEqual(['inst-42/forms/a.pdf', 'inst-42/forms/a.pdf']);
  });

  it('never creates the bucket unless a custom endpoint is configured', async () => {
    await store({ client: fake() }).putFile('a', 'x', {
      contentType: 'text/plain'
    });
    const names = mockSend.mock.calls.map((c) => c[0].constructor.name);
    expect(names).toEqual(['PutObjectCommand']);

    mockSend.mockReset().mockRejectedValueOnce(new Error('404'));
    mockSend.mockResolvedValue({});
    await store({
      client: fake(),
      clientConfig: { endpoint: 'http://localhost:9000' }
    }).putFile('a', 'x', { contentType: 'text/plain' });
    expect(mockSend.mock.calls.map((c) => c[0].constructor.name)).toEqual([
      'HeadBucketCommand',
      'CreateBucketCommand',
      'PutObjectCommand'
    ]);
  });

  it('stores files raw, with content type and an attachment name', async () => {
    await store({ client: fake() }).putFile('r.pdf', Buffer.from('%PDF'), {
      contentType: 'application/pdf',
      filename: 'Résumé "final".pdf'
    });
    const input = mockSend.mock.calls[0][0].input;
    expect(input.Body).toEqual(Buffer.from('%PDF'));
    expect(input.ContentType).toBe('application/pdf');
    expect(input.ContentDisposition).toBe(
      `attachment; filename="R_sum_ _final_.pdf"; filename*=UTF-8''R%C3%A9sum%C3%A9%20%22final%22.pdf`
    );
  });

  it('uploads streams in parts rather than one request', async () => {
    mockSend.mockResolvedValue({ ETag: '"e"' });
    const client = signingClient();
    client.send = mockSend as unknown as S3Client['send'];
    await store({ client }).putFile(
      'big.bin',
      Readable.from([Buffer.from('chunk')]),
      { contentType: 'application/octet-stream' }
    );
    const names = mockSend.mock.calls.map((c) => c[0].constructor.name);
    expect(names).toContain('PutObjectCommand');
    expect(mockSend.mock.calls[0][0].input.Key).toBe('big.bin');
  });

  it('presigns a POST that enforces size, type and key', async () => {
    const s = store({ client: signingClient(), prefix: 'inst-42' });
    const grant = await s.presignUpload('forms/a.pdf', {
      contentType: 'application/pdf',
      maxBytes: 5_000_000
    });
    expect(grant.url).toContain('files');
    expect(grant.key).toBe('forms/a.pdf');
    expect(grant.fields.key).toBe('inst-42/forms/a.pdf');
    expect(grant.fields['Content-Type']).toBe('application/pdf');
    const policy = JSON.parse(
      Buffer.from(grant.fields.Policy, 'base64').toString('utf8')
    );
    expect(policy.conditions).toEqual(
      expect.arrayContaining([
        ['content-length-range', 1, 5_000_000],
        ['eq', '$Content-Type', 'application/pdf']
      ])
    );
    const lifetime = (Date.parse(policy.expiration) - Date.now()) / 1000;
    expect(lifetime).toBeGreaterThan(890);
    expect(lifetime).toBeLessThanOrEqual(900);
  });

  it('refuses link lifetimes over the platform cap', async () => {
    const s = store({
      client: signingClient(),
      presignLimits: { maxUploadSeconds: 60, maxDownloadSeconds: 30 }
    });
    await expect(
      s.presignUpload('a', {
        contentType: 'text/plain',
        maxBytes: 10,
        expiresIn: 61
      })
    ).rejects.toThrow('exceeds the cap of 60s');
    await expect(s.presignDownload('a', { expiresIn: 31 })).rejects.toThrow(
      'S3_PRESIGN_MAX_DOWNLOAD_SECONDS'
    );
    await expect(
      s.presignUpload('a', { contentType: 'text/plain', maxBytes: 0 })
    ).rejects.toThrow('maxBytes');
  });

  it('presigns a download under the prefix with the capped default lifetime', async () => {
    const url = new URL(
      await store({ client: signingClient(), prefix: 'inst-42' }).presignDownload(
        'forms/a.pdf',
        { filename: 'a.pdf' }
      )
    );
    expect(url.pathname).toContain('inst-42/forms/a.pdf');
    expect(url.searchParams.get('X-Amz-Expires')).toBe('300');
    expect(url.searchParams.get('response-content-disposition')).toContain(
      'a.pdf'
    );
  });
});

describe('s3ClientConfig', () => {
  it('passes no credentials when keys are absent (task role)', () => {
    expect(s3ClientConfig({ region: 'us-west-2' })).toEqual({
      region: 'us-west-2'
    });
    expect(
      s3ClientConfig({ region: 'us-west-2', accessKeyId: 'only-one' })
    ).toEqual({ region: 'us-west-2' });
  });

  it('uses static keys and path-style addressing for a custom endpoint', () => {
    expect(
      s3ClientConfig({
        url: 'http://localhost:9000',
        region: 'us-east-1',
        accessKeyId: 'minioadmin',
        secretAccessKey: 'minioadmin'
      })
    ).toEqual({
      region: 'us-east-1',
      endpoint: 'http://localhost:9000',
      forcePathStyle: true,
      credentials: { accessKeyId: 'minioadmin', secretAccessKey: 'minioadmin' }
    });
  });
});

/**
 * Covers the Twilio delivery-status callback mapping: MessageStatus /
 * MessageSid / ErrorMessage from a form-encoded callback are mapped onto the
 * persisted record's status, resolved against an app-discovered entity —
 * mirroring how a consuming blueprint application wires the service.
 */
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { SmsStatusEnum } from '@forklaunch/implementation-messaging-base/enum';
import { EntityManager, InferEntity } from '@mikro-orm/core';
import { MikroORM } from '@mikro-orm/sqlite';
import { v4 } from 'uuid';
import {
  mapTwilioMessageStatus,
  TwilioSmsService
} from '../services/sms.service';

// The application's own entity definition — the ONLY entity the ORM
// discovers, mirroring the blueprint app's persistence layer.
const SmsRecord = defineComplianceEntity({
  name: 'SmsRecord',
  properties: {
    id: fp
      .uuid()
      .primary()
      .onCreate(() => v4())
      .compliance('none'),
    to: fp.string().compliance('none'),
    body: fp.string().compliance('none'),
    status: fp.enum().compliance('none'),
    providerMessageId: fp.string().nullable().compliance('none'),
    error: fp.string().nullable().compliance('none'),
    metadata: fp.json<unknown>().nullable().compliance('none')
  }
});

type SmsRecordEntity = InferEntity<typeof SmsRecord>;

const noopOtel = {
  info: () => {},
  warn: () => {},
  error: () => {},
  debug: () => {}
};

const mappers = {
  SmsRecordMapper: {
    entity: SmsRecord,
    toDto: async (entity: SmsRecordEntity) => ({
      id: entity.id,
      to: entity.to,
      body: entity.body,
      status: entity.status as SmsStatusEnum,
      providerMessageId: entity.providerMessageId ?? undefined,
      error: entity.error ?? undefined,
      metadata: (entity.metadata ?? undefined) as
        | Record<string, unknown>
        | undefined
    })
  },
  SendSmsMapper: {
    entity: SmsRecord,
    toEntity: async (
      dto: { to: string; body: string; metadata?: Record<string, unknown> },
      em: EntityManager
    ) =>
      em.create(SmsRecord, {
        to: dto.to,
        body: dto.body,
        status: SmsStatusEnum.QUEUED,
        providerMessageId: null,
        error: null,
        metadata: dto.metadata ?? null
      })
  }
};

describe('twilio status callback mapping', () => {
  let orm: Awaited<ReturnType<typeof MikroORM.init>>;

  beforeAll(async () => {
    orm = await MikroORM.init({
      dbName: ':memory:',
      entities: [SmsRecord],
      allowGlobalContext: true
    });
    await orm.schema.create();
  });

  afterAll(async () => {
    await orm.close();
  });

  const makeService = (em: EntityManager) =>
    new TwilioSmsService(
      'ACtest',
      'test-auth-token',
      '+15005550006',
      em,
      noopOtel as never,
      null as never,
      mappers as never
    );

  it('maps twilio message statuses onto SmsStatusEnum', () => {
    expect(mapTwilioMessageStatus('queued')).toBe(SmsStatusEnum.QUEUED);
    expect(mapTwilioMessageStatus('accepted')).toBe(SmsStatusEnum.QUEUED);
    expect(mapTwilioMessageStatus('sending')).toBe(SmsStatusEnum.QUEUED);
    expect(mapTwilioMessageStatus('sent')).toBe(SmsStatusEnum.SENT);
    expect(mapTwilioMessageStatus('delivered')).toBe(SmsStatusEnum.DELIVERED);
    expect(mapTwilioMessageStatus('read')).toBe(SmsStatusEnum.DELIVERED);
    expect(mapTwilioMessageStatus('undelivered')).toBe(
      SmsStatusEnum.UNDELIVERED
    );
    expect(mapTwilioMessageStatus('failed')).toBe(SmsStatusEnum.FAILED);
    expect(mapTwilioMessageStatus('canceled')).toBe(SmsStatusEnum.FAILED);
    expect(mapTwilioMessageStatus('anything-unknown')).toBe(
      SmsStatusEnum.QUEUED
    );
  });

  it('sends via the twilio REST api with basic auth and persists the record', async () => {
    const fetchMock = vi.fn(
      async () =>
        new Response(
          JSON.stringify({ sid: 'SM_test_1', status: 'queued' }),
          { status: 201 }
        )
    );
    vi.stubGlobal('fetch', fetchMock);
    try {
      const em = orm.em.fork() as unknown as EntityManager;
      const service = makeService(em);

      const smsRecord = await service.sendSms({
        to: '+15555550100',
        body: 'hello from the test'
      });

      expect(fetchMock).toHaveBeenCalledTimes(1);
      const [url, init] = fetchMock.mock.calls[0] as unknown as [
        string,
        RequestInit
      ];
      expect(url).toBe(
        'https://api.twilio.com/2010-04-01/Accounts/ACtest/Messages.json'
      );
      const headers = init.headers as Record<string, string>;
      expect(headers.Authorization).toBe(
        `Basic ${Buffer.from('ACtest:test-auth-token').toString('base64')}`
      );
      expect(headers['Content-Type']).toBe(
        'application/x-www-form-urlencoded'
      );
      const params = new URLSearchParams(init.body as string);
      expect(params.get('To')).toBe('+15555550100');
      expect(params.get('From')).toBe('+15005550006');
      expect(params.get('Body')).toBe('hello from the test');

      expect(smsRecord.status).toBe(SmsStatusEnum.QUEUED);
      expect(smsRecord.providerMessageId).toBe('SM_test_1');
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('marks the record failed when the twilio api rejects the message', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(JSON.stringify({ message: 'invalid number' }), {
            status: 400
          })
      )
    );
    try {
      const em = orm.em.fork() as unknown as EntityManager;
      const service = makeService(em);

      const smsRecord = await service.sendSms({
        to: 'not-a-number',
        body: 'hello'
      });

      expect(smsRecord.status).toBe(SmsStatusEnum.FAILED);
      expect(smsRecord.error).toContain('Twilio API error (400)');
      expect(smsRecord.providerMessageId).toBeUndefined();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('maps a delivered status callback onto the record', async () => {
    const em = orm.em.fork() as unknown as EntityManager;
    em.create(SmsRecord, {
      to: '+15555550100',
      body: 'callback target',
      status: SmsStatusEnum.SENT,
      providerMessageId: 'SM_callback_1',
      error: null,
      metadata: null
    });
    await em.flush();

    const service = makeService(em);
    const smsRecord = await service.processStatusCallback({
      MessageSid: 'SM_callback_1',
      MessageStatus: 'delivered'
    });

    expect(smsRecord.status).toBe(SmsStatusEnum.DELIVERED);
    expect(smsRecord.error).toBeUndefined();
  });

  it('maps a failed status callback with an error message onto the record', async () => {
    const em = orm.em.fork() as unknown as EntityManager;
    em.create(SmsRecord, {
      to: '+15555550100',
      body: 'callback target',
      status: SmsStatusEnum.SENT,
      providerMessageId: 'SM_callback_2',
      error: null,
      metadata: null
    });
    await em.flush();

    const service = makeService(em);
    const smsRecord = await service.processStatusCallback({
      MessageSid: 'SM_callback_2',
      MessageStatus: 'undelivered',
      ErrorMessage: 'Unreachable destination handset'
    });

    expect(smsRecord.status).toBe(SmsStatusEnum.UNDELIVERED);
    expect(smsRecord.error).toBe('Unreachable destination handset');
  });
});

import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  createSmsClient,
  SmsRequestError,
  SmsValidationError,
  smsSegments
} from '../../src/http/smsClient';
import { type Harness, startHarness } from './harness';

/**
 * SMS end to end: the real client, signed, against the real gateway mock over
 * HTTP; the carrier receipts and inbound texts come back as signed platform
 * events that the app-side receiver verifies.
 */
let h: Harness;
let capped: Harness;
beforeAll(async () => {
  [h, capped] = await Promise.all([
    startHarness(),
    startHarness({ MOCK_SMS_MONTHLY_CAP: '2' })
  ]);
});
afterAll(async () => {
  await Promise.all([h?.stop(), capped?.stop()]);
});
beforeEach(async () => {
  await h.reset();
});

async function eventually<T>(read: () => T[], count: number): Promise<T[]> {
  for (let i = 0; i < 50; i++) {
    const got = read();
    if (got.length >= count) return got;
    await new Promise((r) => setTimeout(r, 20));
  }
  return read();
}

async function inbound(harness: Harness, from: string, body: string) {
  const response = await fetch(`${harness.gatewayUrl}/__mock/sms/inbound`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ from, body })
  });
  return (await response.json()) as {
    keyword: string | null;
    optedOut: boolean;
    events: { type: string; status: number | null }[];
  };
}

describe('sms (e2e)', () => {
  it('sends, and the delivery receipt reaches the app verified', async () => {
    const sms = createSmsClient(h.clientOptions);
    const result = await sms.send({
      to: '+14155550123',
      body: 'Your appointment is confirmed. Sign in for details.'
    });
    expect(result.messageId).toMatch(/^mock-sms-/);
    expect(result.segments).toBe(1);

    const [event] = await eventually(() => h.received('sms'), 1);
    expect(event).toMatchObject({
      feature: 'sms',
      type: 'sms.delivered',
      data: { messageId: result.messageId, to: '+14155550123' }
    });
    expect(h.rejected()).toEqual([]);

    const calls = await h.requests('sms');
    expect(calls.at(-1)).toMatchObject({
      method: 'POST',
      path: '/sms/send',
      body: { to: '+14155550123', purpose: 'transactional' }
    });
    const messages = await (
      await fetch(`${h.gatewayUrl}/__mock/sms/messages`)
    ).json();
    expect(messages).toHaveLength(1);
  });

  it('reports an unreachable handset as sms.failed', async () => {
    const sms = createSmsClient(h.clientOptions);
    const { messageId } = await sms.send({
      to: '+14155550000',
      body: 'Reminder: your visit is tomorrow.'
    });
    const [event] = await eventually(() => h.received('sms'), 1);
    expect(event.type).toBe('sms.failed');
    expect(event.data).toMatchObject({ messageId, to: '+14155550000' });
    expect(String(event.data.reason)).toMatch(/UNREACHABLE/);
  });

  it('relays an inbound text as sms.received', async () => {
    const reply = await inbound(h, '+14155550199', 'Running 10 min late');
    expect(reply.events).toEqual([{ type: 'sms.received', status: 200 }]);
    const [event] = await eventually(() => h.received('sms'), 1);
    expect(event.type).toBe('sms.received');
    expect(event.data).toMatchObject({
      from: '+14155550199',
      body: 'Running 10 min late'
    });
    expect(Number.isNaN(Date.parse(String(event.data.receivedAt)))).toBe(false);
  });

  it('records STOP as an opt-out and refuses the next send with 422', async () => {
    const sms = createSmsClient(h.clientOptions);
    const reply = await inbound(h, '+14155550177', 'stop');
    expect(reply).toMatchObject({ keyword: 'STOP', optedOut: true });
    const events = await eventually(() => h.received('sms'), 2);
    expect(events.map((e) => e.type)).toEqual([
      'sms.received',
      'sms.opted_out'
    ]);
    expect(events[1].data).toMatchObject({ phone: '+14155550177' });

    const refused = await sms
      .send({ to: '+14155550177', body: 'Hello again' })
      .catch((e) => e);
    expect(refused).toBeInstanceOf(SmsRequestError);
    expect(refused.status).toBe(422);

    // START opts back in.
    await inbound(h, '+14155550177', 'START');
    await expect(
      sms.send({ to: '+14155550177', body: 'Welcome back' })
    ).resolves.toMatchObject({ segments: 1 });
  });

  it('refuses past the monthly cap with 429 and a retry-after', async () => {
    const sms = createSmsClient(capped.clientOptions);
    await sms.send({ to: '+14155550101', body: 'one' });
    await sms.send({ to: '+14155550102', body: 'two' });
    const refused = await sms
      .send({ to: '+14155550103', body: 'three' })
      .catch((e) => e);
    expect(refused).toBeInstanceOf(SmsRequestError);
    expect(refused.status).toBe(429);
    expect(refused.retryAfterSeconds).toBeGreaterThan(0);
    expect(refused.message).toMatch(/Monthly SMS cap/);
  });

  it('refuses a wrong key with 401, and bad input before any call', async () => {
    const wrong = createSmsClient({
      ...h.clientOptions,
      hmacKey: 'not-the-key'
    });
    const refused = await wrong
      .send({ to: '+14155550123', body: 'hi' })
      .catch((e) => e);
    expect(refused).toBeInstanceOf(SmsRequestError);
    expect(refused.status).toBe(401);

    const sms = createSmsClient(h.clientOptions);
    await expect(
      sms.send({ to: '4155550123', body: 'hi' })
    ).rejects.toBeInstanceOf(SmsValidationError);
    await expect(
      sms.send({ to: '+14155550123', body: 'x'.repeat(153 * 10 + 1) })
    ).rejects.toThrow(/11 GSM-7 segments/);
    expect(await h.requests('sms')).toEqual([]);
  });

  it('the mock refuses a non-E.164 number with 400 as the platform does', async () => {
    const { createInstanceGatewayTransport } =
      await import('../../src/http/instanceGateway');
    const transport = createInstanceGatewayTransport(h.clientOptions);
    await expect(
      transport.request('POST', '/sms/send', { to: '555-0123', body: 'hi' })
    ).rejects.toMatchObject({ status: 400 });
  });

  it('counts segments the way carriers bill them', () => {
    expect(smsSegments('a'.repeat(160))).toMatchObject({
      encoding: 'GSM-7',
      segments: 1
    });
    expect(smsSegments('a'.repeat(161)).segments).toBe(2);
    expect(smsSegments('{'.repeat(80)).segments).toBe(1);
    expect(smsSegments('é'.repeat(10)).encoding).toBe('GSM-7');
    expect(smsSegments('ü✓'.repeat(35))).toMatchObject({
      encoding: 'UCS-2',
      segments: 1
    });
    expect(smsSegments('✓'.repeat(71)).segments).toBe(2);
  });

  it('refuses to be created outside managed mode, or on first send when deferred', async () => {
    const saved = { ...process.env };
    delete process.env.PLATFORM_GATEWAY_URL;
    delete process.env.INSTANCE_ID;
    delete process.env.INSTANCE_HMAC_KEY;
    try {
      expect(() => createSmsClient()).toThrow(/managed instance/);
      const deferred = createSmsClient({
        gatewayUrl: undefined,
        instanceId: undefined,
        hmacKey: undefined,
        deferRefusal: true
      });
      await expect(
        deferred.send({ to: '+14155550123', body: 'hi' })
      ).rejects.toThrow(/gateway mock/);
    } finally {
      process.env = saved;
    }
  });
});

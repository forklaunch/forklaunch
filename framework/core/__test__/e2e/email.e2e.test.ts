import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  createEmailClient,
  type EmailEventData,
  EmailRequestError
} from '../../src/http/emailClient';
import { type Harness, startHarness } from './harness';

/**
 * Email end to end: the real client, signed, against the real gateway mock
 * over HTTP; delivery events come back signed and are verified by the
 * app-side receiver exactly as a generated service verifies them.
 */
const QUOTA = 5;
let h: Harness;

beforeAll(async () => {
  h = await startHarness({ MOCK_EMAIL_DAILY_QUOTA: String(QUOTA) });
});
afterAll(async () => {
  await h?.stop();
});
beforeEach(async () => {
  await h.reset();
});

async function eventually<T>(
  read: () => T,
  done: (v: T) => boolean
): Promise<T> {
  for (let i = 0; i < 50; i++) {
    const value = read();
    if (done(value)) return value;
    await new Promise((r) => setTimeout(r, 50));
  }
  return read();
}

const emailEvents = () =>
  h.received('email') as unknown as {
    id: string;
    type: string;
    data: EmailEventData;
  }[];

async function messages(to?: string) {
  const q = to ? `?to=${encodeURIComponent(to)}` : '';
  return (await (
    await fetch(`${h.gatewayUrl}/__mock/email/messages${q}`)
  ).json()) as {
    messageId: string;
    from: string;
    to: string[];
    subject: string;
    configurationSet: string;
  }[];
}

describe('email (e2e)', () => {
  it('sends, records the message, and delivers a verified email.delivered', async () => {
    const email = createEmailClient(h.clientOptions);
    const { messageId } = await email.send({
      to: 'pat@example.com',
      subject: 'Your appointment is confirmed',
      text: 'See you Tuesday.',
      tags: { kind: 'appointment' }
    });
    expect(messageId).toMatch(/^mock-/);

    const [recorded] = await messages('pat@example.com');
    expect(recorded).toMatchObject({
      messageId,
      to: ['pat@example.com'],
      subject: 'Your appointment is confirmed',
      from: 'no-reply@e2e-instance.mail.forklaunch.test',
      configurationSet: 'fl-e2e-instance'
    });

    const events = await eventually(emailEvents, (e) => e.length >= 1);
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({
      type: 'email.delivered',
      data: {
        messageId,
        recipients: ['pat@example.com'],
        tags: { kind: 'appointment' }
      }
    });
    expect(h.rejected()).toEqual([]);
    expect(await email.suppressed('pat@example.com')).toBe(false);
  });

  it('a bounce is reported, suppresses the address, and the next send is refused', async () => {
    const email = createEmailClient(h.clientOptions);
    const { messageId } = await email.send({
      to: 'bounce@bounce.test',
      subject: 'Welcome',
      html: '<p>Hello</p>'
    });
    const events = await eventually(emailEvents, (e) => e.length >= 1);
    expect(events[0]).toMatchObject({
      type: 'email.bounced',
      data: {
        messageId,
        recipients: ['bounce@bounce.test'],
        bounceType: 'Permanent',
        permanent: true
      }
    });
    expect(await email.suppressed('bounce@bounce.test')).toBe(true);
    expect(await email.suppressed('Bounce@Bounce.test')).toBe(true);

    const refused = await email
      .send({ to: 'bounce@bounce.test', subject: 'Again', text: 'x' })
      .catch((e) => e);
    expect(refused).toBeInstanceOf(EmailRequestError);
    expect(refused.status).toBe(422);
    expect((await messages('bounce@bounce.test')).length).toBe(1);
  });

  it('a complaint follows delivery and suppresses the address', async () => {
    const email = createEmailClient(h.clientOptions);
    const { messageId } = await email.send({
      to: 'complaint@example.com',
      subject: 'Newsletter',
      text: 'Hi'
    });
    const events = await eventually(emailEvents, (e) => e.length >= 2);
    expect(events.map((e) => e.type)).toEqual([
      'email.delivered',
      'email.complained'
    ]);
    expect(events[1].data).toMatchObject({ messageId, feedbackType: 'abuse' });
    expect(new Set(events.map((e) => e.id)).size).toBe(2);
    expect(await email.suppressed('complaint@example.com')).toBe(true);
  });

  it('refuses past the daily quota with 429 and a retry-after', async () => {
    const email = createEmailClient(h.clientOptions);
    for (let i = 0; i < QUOTA; i++) {
      await email.send({
        to: `user${i}@example.com`,
        subject: 'Hi',
        text: 'x'
      });
    }
    const refused = await email
      .send({ to: 'one-more@example.com', subject: 'Hi', text: 'x' })
      .catch((e) => e);
    expect(refused).toBeInstanceOf(EmailRequestError);
    expect(refused.status).toBe(429);
    expect(refused.retryAfterSeconds).toBeGreaterThan(0);
  });

  it('refuses bad input before sending, and the mock refuses it too', async () => {
    const email = createEmailClient(h.clientOptions);
    for (const bad of [
      { to: 'not-an-address', subject: 'Hi', text: 'x' },
      { to: 'a@example.com', subject: 'Line\nbreak', text: 'x' },
      { to: 'a@example.com', subject: 'No body' },
      {
        to: 'a@example.com',
        subject: 'Hi',
        text: 'x',
        tags: { 'fl-instance': 'x' }
      }
    ]) {
      const error = await email.send(bad).catch((e) => e);
      expect(error).toBeInstanceOf(EmailRequestError);
      expect(error.status).toBe(400);
    }
    expect(await h.requests('email')).toEqual([]);

    // A client that skips validation meets the same rule at the gateway.
    const { createInstanceGatewayTransport } =
      await import('../../src/http/instanceGateway');
    const raw = createInstanceGatewayTransport(h.clientOptions);
    const answer = await raw
      .request('POST', '/email/send', { to: 'nope', subject: 'x', text: 'x' })
      .catch((e) => e);
    expect(answer.status).toBe(400);
  });

  it('refuses a wrong key with 401, and refuses to exist outside managed mode', async () => {
    const wrong = createEmailClient({
      ...h.clientOptions,
      hmacKey: 'not-the-key'
    });
    const error = await wrong
      .send({ to: 'pat@example.com', subject: 'Hi', text: 'x' })
      .catch((e) => e);
    expect(error).toBeInstanceOf(EmailRequestError);
    expect(error.status).toBe(401);
    expect(await messages()).toEqual([]);

    const saved = { ...process.env };
    delete process.env.PLATFORM_GATEWAY_URL;
    delete process.env.INSTANCE_ID;
    delete process.env.INSTANCE_HMAC_KEY;
    try {
      expect(() => createEmailClient()).toThrow(/managed instances/);
    } finally {
      process.env = saved;
    }
  });
});

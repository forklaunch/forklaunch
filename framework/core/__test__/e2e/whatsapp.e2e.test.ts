import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  createWhatsAppClient,
  WhatsAppRequestError
} from '../../src/http/whatsappClient';
import { type Harness, startHarness } from './harness';

/**
 * WhatsApp end to end: the real client, signed, against the real gateway
 * mock over HTTP, with events delivered to and verified by the app side.
 */

const PATIENT = '+14155550123';

async function until<T>(read: () => T | undefined, ms = 3000): Promise<T> {
  const deadline = Date.now() + ms;
  for (;;) {
    const value = read();
    if (value !== undefined) return value;
    if (Date.now() > deadline) throw new Error('timed out waiting');
    await new Promise((r) => setTimeout(r, 25));
  }
}

async function inbound(h: Harness, body: Record<string, unknown>) {
  const response = await fetch(`${h.gatewayUrl}/__mock/whatsapp/inbound`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body)
  });
  return { status: response.status, body: await response.json() };
}

async function refusal(
  promise: Promise<unknown>
): Promise<WhatsAppRequestError> {
  const error = await promise.then(
    () => undefined,
    (e: unknown) => e
  );
  expect(error).toBeInstanceOf(WhatsAppRequestError);
  return error as WhatsAppRequestError;
}

describe('whatsapp (e2e)', () => {
  let h: Harness;
  beforeAll(async () => {
    h = await startHarness({
      MOCK_WHATSAPP_TEMPLATES:
        'appointment_reminder:en_US:APPROVED:UTILITY,appointment_reminder:es_MX:APPROVED:UTILITY,promo_fall:en_US:PENDING:MARKETING'
    });
  });
  afterAll(async () => {
    await h?.stop();
  });
  beforeEach(async () => {
    await h.reset();
  });

  it('lists the templates with their review status', async () => {
    const whatsapp = createWhatsAppClient(h.clientOptions);
    const templates = await whatsapp.templates();
    expect(templates).toEqual([
      {
        name: 'appointment_reminder',
        language: 'en_US',
        status: 'APPROVED',
        category: 'UTILITY'
      },
      {
        name: 'appointment_reminder',
        language: 'es_MX',
        status: 'APPROVED',
        category: 'UTILITY'
      },
      {
        name: 'promo_fall',
        language: 'en_US',
        status: 'PENDING',
        category: 'MARKETING'
      }
    ]);
    expect((await h.requests('whatsapp')).at(-1)).toMatchObject({
      method: 'GET',
      path: '/whatsapp/templates'
    });
  });

  it('sends a template and the app receives a verified delivered status', async () => {
    const whatsapp = createWhatsAppClient(h.clientOptions);
    const { messageId } = await whatsapp.sendTemplate({
      to: PATIENT,
      template: 'appointment_reminder',
      language: 'en_US',
      components: [
        { type: 'body', parameters: [{ type: 'text', text: 'Tuesday 3pm' }] }
      ]
    });
    expect(messageId).toMatch(/^wamid\./);

    const delivered = await until(() =>
      h
        .received('whatsapp')
        .find(
          (e) => e.type === 'whatsapp.status' && e.data.status === 'delivered'
        )
    );
    expect(delivered.data).toMatchObject({
      messageId,
      status: 'delivered',
      recipient: PATIENT
    });
    expect(h.received('whatsapp').map((e) => e.data.status)).toEqual([
      'sent',
      'delivered'
    ]);
    expect(h.rejected()).toEqual([]);

    const [call] = await h.requests('whatsapp');
    expect(call).toMatchObject({
      method: 'POST',
      path: '/whatsapp/messages',
      body: {
        to: PATIENT,
        type: 'template',
        template: 'appointment_reminder',
        language: 'en_US'
      }
    });
  });

  it('refuses unknown, untranslated and unapproved templates', async () => {
    const whatsapp = createWhatsAppClient(h.clientOptions);
    const unknown = await refusal(
      whatsapp.sendTemplate({
        to: PATIENT,
        template: 'nope',
        language: 'en_US'
      })
    );
    expect(unknown.status).toBe(400);
    expect(unknown.message).toContain("Unknown template 'nope'");
    const language = await refusal(
      whatsapp.sendTemplate({
        to: PATIENT,
        template: 'appointment_reminder',
        language: 'fr_FR'
      })
    );
    expect(language.status).toBe(400);
    const pending = await refusal(
      whatsapp.sendTemplate({
        to: PATIENT,
        template: 'promo_fall',
        language: 'en_US'
      })
    );
    expect(pending.status).toBe(400);
    expect(pending.message).toContain('PENDING');
    // A bad number never leaves the instance.
    const bad = await refusal(
      whatsapp.sendTemplate({
        to: '4155550123',
        template: 'appointment_reminder',
        language: 'en_US'
      })
    );
    expect(bad.status).toBe(400);
    expect(await h.requests('whatsapp')).toHaveLength(3);
  });

  it('refuses free text outside the 24-hour window, and allows it after an inbound message', async () => {
    const whatsapp = createWhatsAppClient(h.clientOptions);
    const closed = await refusal(
      whatsapp.sendText({ to: PATIENT, body: 'Hi there' })
    );
    expect(closed.status).toBe(422);
    expect(closed.message).toContain('24 hours');

    // An inbound message 25 hours ago does not open the window.
    const stale = await inbound(h, {
      from: PATIENT,
      text: 'old',
      receivedAt: new Date(Date.now() - 25 * 3600_000).toISOString()
    });
    expect(stale.body.delivered).toBe(200);
    expect(
      (await refusal(whatsapp.sendText({ to: PATIENT, body: 'Hi' }))).status
    ).toBe(422);

    const fresh = await inbound(h, {
      from: PATIENT,
      text: 'Can I move my appointment?'
    });
    expect(fresh.status).toBe(200);
    const received = h
      .received('whatsapp')
      .filter((e) => e.type === 'whatsapp.received');
    expect(received).toHaveLength(2);
    expect(received[1].data).toMatchObject({
      from: PATIENT,
      text: 'Can I move my appointment?',
      type: 'text'
    });
    expect(typeof received[1].data.receivedAt).toBe('string');

    const { messageId } = await whatsapp.sendText({
      to: PATIENT,
      body: 'Yes — which day works?'
    });
    expect(messageId).toMatch(/^wamid\./);
    await until(() =>
      h
        .received('whatsapp')
        .find(
          (e) =>
            e.type === 'whatsapp.status' &&
            e.data.messageId === messageId &&
            e.data.status === 'delivered'
        )
    );
    // The window is per recipient.
    expect(
      (await refusal(whatsapp.sendText({ to: '+14155550999', body: 'Hi' })))
        .status
    ).toBe(422);
  });

  it('refuses a wrong key with 401', async () => {
    const whatsapp = createWhatsAppClient({
      ...h.clientOptions,
      hmacKey: 'not-the-key'
    });
    const error = await refusal(whatsapp.templates());
    expect(error.status).toBe(401);
    expect(await h.requests('whatsapp')).toHaveLength(0);
  });

  it('refuses to be created outside managed mode', () => {
    expect(() =>
      createWhatsAppClient({
        gatewayUrl: undefined,
        instanceId: undefined,
        hmacKey: undefined
      })
    ).toThrow(/managed mode/);
  });
});

describe('whatsapp for a HIPAA product (e2e)', () => {
  let h: Harness;
  beforeAll(async () => {
    h = await startHarness({ MOCK_WHATSAPP_HIPAA: '1' });
  });
  afterAll(async () => {
    await h?.stop();
  });

  it('refuses every send with 403 and delivers nothing', async () => {
    const whatsapp = createWhatsAppClient(h.clientOptions);
    const template = await refusal(
      whatsapp.sendTemplate({
        to: PATIENT,
        template: 'appointment_reminder',
        language: 'en_US'
      })
    );
    expect(template.status).toBe(403);
    expect(template.message).toContain('BAA');
    await inbound(h, { from: PATIENT, text: 'hello' });
    expect(
      (await refusal(whatsapp.sendText({ to: PATIENT, body: 'hi' }))).status
    ).toBe(403);
    expect(
      h.received('whatsapp').filter((e) => e.type === 'whatsapp.status')
    ).toEqual([]);
  });
});

describe('whatsapp with no phone number linked (e2e)', () => {
  let h: Harness;
  beforeAll(async () => {
    h = await startHarness({ MOCK_WHATSAPP_UNCONFIGURED: '1' });
  });
  afterAll(async () => {
    await h?.stop();
  });

  it('answers 409 with what to do', async () => {
    const error = await refusal(
      createWhatsAppClient(h.clientOptions).templates()
    );
    expect(error.status).toBe(409);
    expect(error.message).toContain('phone number');
  });
});

describe('whatsapp rate limit (e2e)', () => {
  let h: Harness;
  beforeAll(async () => {
    h = await startHarness({ MOCK_WHATSAPP_RPM: '2' });
  });
  afterAll(async () => {
    await h?.stop();
  });

  it('answers 429 with retry-after past the per-minute limit', async () => {
    const whatsapp = createWhatsAppClient(h.clientOptions);
    const send = () =>
      whatsapp.sendTemplate({
        to: PATIENT,
        template: 'appointment_reminder',
        language: 'en_US'
      });
    await send();
    await send();
    const limited = await refusal(send());
    expect(limited.status).toBe(429);
    expect(limited.retryAfterSeconds).toBeGreaterThan(0);
  });
});

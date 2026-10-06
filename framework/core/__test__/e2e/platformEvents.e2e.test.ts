import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import {
  createInstanceGatewayTransport,
  InstanceGatewayRequestError,
  signPlatformEvent,
  verifyPlatformEvent
} from '../../src/http/instanceGateway';
import { createModelGatewayClient } from '../../src/http/modelGatewayClient';
import { type Harness, HMAC_KEY, startHarness } from './harness';

/**
 * The shared plumbing every managed feature stands on: signed calls out to
 * the gateway, and signed events back in.
 */
let h: Harness;
beforeAll(async () => {
  h = await startHarness();
});
afterAll(async () => {
  await h?.stop();
});

describe('instance gateway plumbing (e2e)', () => {
  it('delivers a signed event the app verifies', async () => {
    const delivery = await h.triggerEvent('payments', 'test.ping', { n: 1 });
    expect(delivery.status).toBe(200);
    const [event] = h.received('payments');
    expect(event).toMatchObject({ feature: 'payments', type: 'test.ping', data: { n: 1 } });
    expect(event.id).toMatch(/^evt_mock_/);
  });

  it('records signed gateway calls and refuses a wrong key', async () => {
    await createModelGatewayClient(h.clientOptions).models();
    const calls = await h.requests('models');
    expect(calls.at(-1)).toMatchObject({ method: 'GET', path: '/models' });
    const wrong = createInstanceGatewayTransport({ ...h.clientOptions, hmacKey: 'nope' });
    await expect(wrong.request('GET', '/models')).rejects.toBeInstanceOf(
      InstanceGatewayRequestError
    );
  });

  it('refuses forged, stale and replayed events', () => {
    const body = JSON.stringify({ id: 'e1', feature: 'email', type: 'email.bounced', occurredAt: '', data: {} });
    const path = '/platform-events/email';
    const { authorization } = signPlatformEvent({ hmacKey: HMAC_KEY, path, body });
    const req = { method: 'POST', path, headers: { authorization }, body };
    expect(verifyPlatformEvent(req, { hmacKey: HMAC_KEY }).type).toBe('email.bounced');
    expect(() => verifyPlatformEvent(req, { hmacKey: HMAC_KEY })).toThrow('Replayed');
    expect(() => verifyPlatformEvent({ ...req, body: body.replace('bounced', 'opened') }, { hmacKey: HMAC_KEY })).toThrow('Invalid signature');
    const fresh = signPlatformEvent({ hmacKey: HMAC_KEY, path, body });
    expect(() =>
      verifyPlatformEvent({ ...req, headers: fresh }, { hmacKey: HMAC_KEY, now: () => Date.now() + 10 * 60_000 })
    ).toThrow('Stale');
    const instanceSigned = { authorization: fresh.authorization.replace('keyId=platform', 'keyId=someone') };
    expect(() => verifyPlatformEvent({ ...req, headers: instanceSigned }, { hmacKey: HMAC_KEY })).toThrow('Not signed by the platform');
  });
});

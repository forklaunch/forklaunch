import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  createVoiceClient,
  VoiceRequestError
} from '../../src/http/voiceClient';
import { type Harness, startHarness } from './harness';

/**
 * Voice end to end: the real client, signed, against the real gateway mock
 * over HTTP, with call events delivered back to an app-side receiver that
 * verifies them as a generated service does.
 */

async function until<T>(
  read: () => T,
  done: (value: T) => boolean,
  timeoutMs = 5000
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = read();
    if (done(value)) return value;
    if (Date.now() > deadline) return value;
    await new Promise((r) => setTimeout(r, 25));
  }
}

async function refusal(promise: Promise<unknown>): Promise<VoiceRequestError> {
  try {
    await promise;
  } catch (error) {
    expect(error).toBeInstanceOf(VoiceRequestError);
    return error as VoiceRequestError;
  }
  throw new Error('expected a refusal');
}

describe('voice (e2e): calls that end by themselves', () => {
  let h: Harness;
  beforeAll(async () => {
    h = await startHarness({
      MOCK_VOICE_CALL_MS: '300',
      MOCK_VOICE_RECORD: '1'
    });
  });
  afterAll(async () => {
    await h?.stop();
  });
  beforeEach(async () => {
    await h.reset();
  });

  it('places a call; started, ended and recording events arrive verified, in order', async () => {
    const voice = createVoiceClient(h.clientOptions);
    const { callId } = await voice.startOutboundCall({
      to: '+15551230000',
      flow: 'appointment_reminder',
      attributes: { appointmentId: 'appt_123' }
    });
    expect(callId).toMatch(/^mock-call-/);

    const inProgress = await voice.call(callId);
    expect(inProgress.status).toBe('in_progress');
    expect(inProgress.endedAt).toBeUndefined();

    const events = await until(
      () => h.received('voice'),
      (e) => e.length >= 3
    );
    expect(events.map((e) => e.type)).toEqual([
      'voice.call.started',
      'voice.call.ended',
      'voice.recording.ready'
    ]);
    expect(events[0].data).toMatchObject({
      callId,
      flow: 'appointment_reminder'
    });
    expect(events[1].data).toEqual({
      callId,
      durationSeconds: 42,
      disconnectReason: 'customer'
    });
    expect(events[2].data).toEqual({
      callId,
      recordingKey: `voice/recordings/${callId}.wav`
    });
    expect(h.rejected()).toEqual([]);

    const ended = await voice.call(callId);
    expect(ended).toMatchObject({
      status: 'ended',
      durationSeconds: 42,
      disconnectReason: 'customer'
    });
    expect(ended.endedAt).toBeTruthy();

    const [request] = await h.requests('voice');
    expect(request).toMatchObject({
      method: 'POST',
      path: '/voice/calls',
      body: { to: '+15551230000', flow: 'appointment_reminder' }
    });
  });

  it('reports a busy number as ended with reason busy', async () => {
    const voice = createVoiceClient(h.clientOptions);
    const { callId } = await voice.startOutboundCall({
      to: '+15551239999',
      flow: 'appointment_reminder'
    });
    const events = await until(
      () => h.received('voice'),
      (e) => e.length >= 2
    );
    expect(events.map((e) => e.type)).toEqual([
      'voice.call.started',
      'voice.call.ended'
    ]);
    expect(events[1].data).toEqual({
      callId,
      durationSeconds: 0,
      disconnectReason: 'busy'
    });
    expect((await voice.call(callId)).status).toBe('ended');
  });

  it('refuses an unknown flow with 400, and an ARN before sending it', async () => {
    const voice = createVoiceClient(h.clientOptions);
    const unknown = await refusal(
      voice.startOutboundCall({ to: '+15551230000', flow: 'lab_results' })
    );
    expect(unknown.status).toBe(400);
    expect(unknown.message).toContain("Unknown flow 'lab_results'");

    const arn = await refusal(
      voice.startOutboundCall({
        to: '+15551230000',
        flow: 'arn:aws:connect:us-east-1:123:instance/x/contact-flow/y'
      })
    );
    expect(arn.status).toBe(400);
    const reserved = await refusal(
      voice.startOutboundCall({
        to: '+15551230000',
        flow: 'appointment_reminder',
        attributes: { fl_instance_id: 'someone-else' }
      })
    );
    expect(reserved.status).toBe(400);
    // Only the unknown flow reached the gateway.
    expect(await h.requests('voice')).toHaveLength(1);
  });

  it('answers 404 for a call this instance did not place', async () => {
    const voice = createVoiceClient(h.clientOptions);
    expect((await refusal(voice.call('mock-call-nope'))).status).toBe(404);
  });

  it('refuses a wrong key with 401', async () => {
    const voice = createVoiceClient({ ...h.clientOptions, hmacKey: 'wrong' });
    const error = await refusal(
      voice.startOutboundCall({
        to: '+15551230000',
        flow: 'appointment_reminder'
      })
    );
    expect(error.status).toBe(401);
    expect(h.received('voice')).toEqual([]);
  });

  it('refuses to be created outside managed mode', () => {
    expect(() =>
      createVoiceClient({ gatewayUrl: '', instanceId: '', hmacKey: '' })
    ).toThrow(/only available to instances hosted in managed mode/);
  });
});

describe('voice (e2e): long calls', () => {
  let h: Harness;
  beforeAll(async () => {
    h = await startHarness({
      MOCK_VOICE_CALL_MS: '60000',
      MOCK_VOICE_MAX_CONCURRENT: '2'
    });
  });
  afterAll(async () => {
    await h?.stop();
  });
  beforeEach(async () => {
    await h.reset();
  });

  it('ends a call early with reason api', async () => {
    const voice = createVoiceClient(h.clientOptions);
    const { callId } = await voice.startOutboundCall({
      to: '+15551230000',
      flow: 'appointment_reminder'
    });
    expect((await voice.call(callId)).status).toBe('in_progress');
    await voice.endCall(callId);

    const events = await until(
      () => h.received('voice'),
      (e) => e.length >= 2
    );
    expect(events.map((e) => e.type)).toEqual([
      'voice.call.started',
      'voice.call.ended'
    ]);
    expect(events[1].data).toMatchObject({ callId, disconnectReason: 'api' });
    expect(
      (events[1].data as { durationSeconds: number }).durationSeconds
    ).toBeGreaterThan(0);

    const ended = await voice.call(callId);
    expect(ended).toMatchObject({ status: 'ended', disconnectReason: 'api' });
    expect((await refusal(voice.endCall(callId))).status).toBe(409);
  });

  it('refuses a call past the concurrency limit with 429, and frees the slot on hang-up', async () => {
    const voice = createVoiceClient(h.clientOptions);
    const first = await voice.startOutboundCall({
      to: '+15551230001',
      flow: 'appointment_reminder'
    });
    await voice.startOutboundCall({
      to: '+15551230002',
      flow: 'appointment_reminder'
    });
    const limited = await refusal(
      voice.startOutboundCall({
        to: '+15551230003',
        flow: 'appointment_reminder'
      })
    );
    expect(limited.status).toBe(429);
    expect(limited.retryAfterSeconds).toBe(1);

    await voice.endCall(first.callId);
    const third = await voice.startOutboundCall({
      to: '+15551230003',
      flow: 'appointment_reminder'
    });
    expect(third.callId).toMatch(/^mock-call-/);
  });
});

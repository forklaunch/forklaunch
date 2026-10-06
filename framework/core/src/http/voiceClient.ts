import {
  createInstanceGatewayTransport,
  type InstanceGatewayOptions,
  InstanceGatewayRequestError,
  type InstanceGatewayTransport
} from './instanceGateway';

/**
 * Client for platform-held voice calls (Amazon Connect), used from inside a
 * managed instance.
 *
 * The instance never holds an AWS credential, a Connect instance id or a
 * contact-flow ARN. It asks the platform to place a call with a FLOW NAME from
 * the product's flow catalog (`appointment_reminder`, …); the platform maps it
 * to the contact flow, dials from the phone number claimed for this instance,
 * and enforces the instance's concurrent-call and monthly-minute limits.
 *
 * Call progress arrives as platform events on `/platform-events/voice`
 * (`voice.call.started`, `voice.call.ended`, `voice.recording.ready`), signed
 * with the instance key; `forklaunch infra add <service> voice` writes the
 * route and a handler stub.
 *
 * Attributes are passed to the contact flow and are stored in Connect's
 * contact records. Put identifiers in them (an appointment id the flow looks
 * up), never health information: the `voice-protected-data` check flags a
 * `.deanon` value flowing into them.
 *
 * Locally, `infra add` points the service at the gateway mock in
 * docker-compose (flows from `MOCK_VOICE_FLOWS`, events delivered back to the
 * service), so the same code runs with no AWS account.
 *
 * @example
 * const voice = createVoiceClient();
 * const { callId } = await voice.startOutboundCall({
 *   to: '+15551230000',
 *   flow: 'appointment_reminder',
 *   attributes: { appointmentId: appt.id }
 * });
 * const { status } = await voice.call(callId);
 */

export type VoiceClientOptions = InstanceGatewayOptions;

/** Limits the gateway enforces on attributes; the client checks them first. */
export const VOICE_ATTRIBUTE_LIMITS = {
  maxKeys: 20,
  maxKeyLength: 64,
  maxValueLength: 256,
  maxTotalBytes: 4096,
  /** Keys the platform reserves (it prefixes and adds its own under `fl_`). */
  reservedPrefix: 'fl_'
} as const;

export interface StartOutboundCallRequest {
  /** The number to call, E.164 (`+15551230000`). */
  to: string;
  /** A flow name from the product's catalog, e.g. `appointment_reminder`. */
  flow: string;
  /** String values the contact flow can read. No health information. */
  attributes?: Record<string, string>;
}

export interface StartOutboundCallResult {
  callId: string;
}

export type VoiceCallStatus = 'initiated' | 'in_progress' | 'ended';

export interface VoiceCall {
  status: VoiceCallStatus;
  startedAt: string;
  endedAt?: string;
  durationSeconds?: number;
  /** Why it ended: `customer`, `agent`, `api`, `busy`, `no_answer`, … */
  disconnectReason?: string;
}

/** Data of `voice.call.started`. */
export interface VoiceCallStartedEvent {
  callId: string;
  flow?: string;
}

/** Data of `voice.call.ended`. */
export interface VoiceCallEndedEvent {
  callId: string;
  durationSeconds: number;
  disconnectReason: string;
}

/** Data of `voice.recording.ready`: the key in the instance's object store. */
export interface VoiceRecordingReadyEvent {
  callId: string;
  recordingKey: string;
}

/** A refusal or failure from the voice gateway, with its status. */
export class VoiceRequestError extends InstanceGatewayRequestError {
  override readonly name = 'VoiceRequestError' as const;
}

const E164 = /^\+[1-9]\d{6,14}$/;
const FLOW_NAME = /^[a-z][a-z0-9_-]{0,63}$/;
const ATTRIBUTE_KEY = /^[A-Za-z][A-Za-z0-9_]*$/;

/** Why a request would be refused, or undefined. Shared with the gateway. */
export function voiceRequestProblem(
  request: StartOutboundCallRequest
): string | undefined {
  if (typeof request.to !== 'string' || !E164.test(request.to)) {
    return '`to` must be an E.164 number such as +15551230000';
  }
  if (typeof request.flow !== 'string' || request.flow.startsWith('arn:')) {
    return '`flow` is a flow name from the product catalog (e.g. appointment_reminder), not a contact-flow ARN';
  }
  if (!FLOW_NAME.test(request.flow)) {
    return '`flow` must be a catalog flow name: lowercase letters, digits, _ or -';
  }
  const attributes = request.attributes ?? {};
  if (typeof attributes !== 'object' || Array.isArray(attributes)) {
    return '`attributes` must be an object of strings';
  }
  const entries = Object.entries(attributes);
  const limits = VOICE_ATTRIBUTE_LIMITS;
  if (entries.length > limits.maxKeys) {
    return `at most ${limits.maxKeys} attributes`;
  }
  let total = 0;
  for (const [key, value] of entries) {
    if (!ATTRIBUTE_KEY.test(key) || key.length > limits.maxKeyLength) {
      return `attribute key '${key}' must be letters, digits or _ (max ${limits.maxKeyLength})`;
    }
    if (key.toLowerCase().startsWith(limits.reservedPrefix)) {
      return `attribute keys starting with '${limits.reservedPrefix}' are reserved for the platform`;
    }
    if (typeof value !== 'string') {
      return `attribute '${key}' must be a string`;
    }
    if (value.length > limits.maxValueLength) {
      return `attribute '${key}' is longer than ${limits.maxValueLength} characters`;
    }
    total += Buffer.byteLength(key) + Buffer.byteLength(value);
  }
  if (total > limits.maxTotalBytes) {
    return `attributes exceed ${limits.maxTotalBytes} bytes`;
  }
  return undefined;
}

/**
 * The voice client. A class so a service's registrations.ts can declare it
 * (`type: VoiceClient`); construct it with `createVoiceClient()`.
 */
export class VoiceClient {
  private readonly transport: InstanceGatewayTransport;

  /** Throws when the process is not a managed instance (see instanceGatewaySettings). */
  constructor(options: VoiceClientOptions = {}) {
    this.transport = createInstanceGatewayTransport(
      options,
      'Voice calls (createVoiceClient)'
    );
  }

  /** Place an outbound call; resolves once the platform accepted it. */
  async startOutboundCall(
    request: StartOutboundCallRequest
  ): Promise<StartOutboundCallResult> {
    const problem = voiceRequestProblem(request);
    if (problem) throw new VoiceRequestError(400, problem);
    return this.send<StartOutboundCallResult>('POST', '/voice/calls', {
      to: request.to,
      flow: request.flow,
      ...(request.attributes ? { attributes: request.attributes } : {})
    });
  }

  /** Hang up a call this instance placed. */
  async endCall(callId: string): Promise<void> {
    await this.send<unknown>('DELETE', callPath(callId));
  }

  /** Where a call this instance placed stands. */
  async call(callId: string): Promise<VoiceCall> {
    return this.send<VoiceCall>('GET', callPath(callId));
  }

  private async send<T>(
    method: 'GET' | 'POST' | 'DELETE',
    route: string,
    body?: Record<string, unknown>
  ): Promise<T> {
    try {
      const response = await this.transport.request(method, route, body);
      const text = await response.text();
      return (text ? JSON.parse(text) : undefined) as T;
    } catch (error) {
      if (error instanceof InstanceGatewayRequestError) {
        throw new VoiceRequestError(
          error.status,
          error.message,
          error.retryAfterSeconds
        );
      }
      throw error;
    }
  }
}

function callPath(callId: string): string {
  if (typeof callId !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(callId)) {
    throw new VoiceRequestError(400, 'callId is not a call id');
  }
  return `/voice/calls/${callId}`;
}

/** A voice client for this managed instance (settings from the environment). */
export function createVoiceClient(
  options: VoiceClientOptions = {}
): VoiceClient {
  return new VoiceClient(options);
}

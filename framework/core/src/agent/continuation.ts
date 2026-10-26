import {
  createCipheriv,
  createDecipheriv,
  createHash,
  randomBytes
} from 'node:crypto';
import type { GatewayChatMessage } from '../http/modelGatewayClient';

export interface PendingToolCall {
  id: string;
  name: string;
  arguments: string;
}

/** Everything needed to resume a run after the user decides on a tool call. */
export interface Continuation {
  v: 1;
  userId: string;
  expiresAt: number;
  steps: number;
  messages: GatewayChatMessage[];
  /** The call awaiting approval first, then any later calls of the same model turn. */
  pending: PendingToolCall[];
  toolCalls: { id: string; name: string; ok: boolean }[];
}

const MAX_TOKEN_BYTES = 256 * 1024;
export const CONTINUATION_TTL_MS = 15 * 60_000;

const key = (secret: string) =>
  createHash('sha256')
    .update(`forklaunch-agent-continuation:${secret}`)
    .digest();

/**
 * Seals a continuation with AES-256-GCM: tool results inside stay confidential,
 * and any change makes it unreadable. Nothing is stored on the server.
 */
export function sealContinuation(value: Continuation, secret: string): string {
  const iv = randomBytes(12);
  const cipher = createCipheriv('aes-256-gcm', key(secret), iv);
  const body = Buffer.concat([
    cipher.update(JSON.stringify(value), 'utf8'),
    cipher.final()
  ]);
  const token = Buffer.concat([iv, cipher.getAuthTag(), body]).toString(
    'base64url'
  );
  if (token.length > MAX_TOKEN_BYTES)
    throw new Error('This run is too long to pause for approval.');
  return token;
}

export function openContinuation(
  token: string,
  secret: string,
  userId: string
): Continuation | null {
  try {
    if (token.length > MAX_TOKEN_BYTES) return null;
    const raw = Buffer.from(token, 'base64url');
    const decipher = createDecipheriv(
      'aes-256-gcm',
      key(secret),
      raw.subarray(0, 12)
    );
    decipher.setAuthTag(raw.subarray(12, 28));
    const value = JSON.parse(
      Buffer.concat([
        decipher.update(raw.subarray(28)),
        decipher.final()
      ]).toString('utf8')
    ) as Continuation;
    if (
      value.v !== 1 ||
      value.userId !== userId ||
      value.expiresAt < Date.now()
    )
      return null;
    return value;
  } catch {
    return null;
  }
}

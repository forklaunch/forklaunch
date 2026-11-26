import { readModelStream } from './modelStream';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import {
  ModelGatewayRequestError,
  type GatewayChatRequest
} from './modelGatewayClient';
import {
  createPreviewModelGatewayClient,
  type PreviewModelGatewayClient
} from './previewModelGatewayClient';

/** Secret, app-scoped capability. Never expose this object to a browser or logger. */
export interface LocalModelCredential {
  grantId: string;
  capability: string;
  gatewayBaseUrl: string;
  model: string;
  expiresAt: number;
  maxCalls: number;
  maxUnits: number;
  scope: 'local-runtime';
}
export interface LocalModelGatewayOptions {
  applicationId?: string;
  model?: string;
  /** A trusted host can provide credentials without making its account token available. */
  credentialProvider?: () => Promise<LocalModelCredential>;
  fetch?: typeof fetch;
}
const execute = promisify(execFile);
const gateway =
  'https://studio-orchestrator.forklaunch.com/studio-orchestrator/desktop-runtime-model-gateway';
function validate(value: LocalModelCredential): LocalModelCredential {
  if (
    !value ||
    !/^[a-f0-9]{64}$/.test(value.grantId) ||
    !/^[a-f0-9]{64}$/.test(value.capability) ||
    value.gatewayBaseUrl !== `${gateway}/${value.grantId}/v1` ||
    value.scope !== 'local-runtime' ||
    !Number.isSafeInteger(value.expiresAt) ||
    value.expiresAt <= Date.now() ||
    value.expiresAt > Date.now() + 900000 ||
    !/^[A-Za-z0-9._-]{1,128}$/.test(value.model) ||
    !Number.isInteger(value.maxCalls) ||
    value.maxCalls < 1 ||
    value.maxCalls > 20 ||
    !Number.isInteger(value.maxUnits) ||
    value.maxUnits < 1 ||
    value.maxUnits > 100000
  ) {
    throw new Error(
      'Invalid local model connection. No model request was sent.'
    );
  }
  return value;
}
/**
 * Server-side local development only. Prefer an injected desktop relay; otherwise
 * ask the installed CLI for an app-scoped credential. The CLI uses an explicit
 * FORKLAUNCH_API_KEY first, then its selected signed-in account. This module never
 * reads the account credential file. Credentials are cached in memory only.
 */
export function createLocalModelGatewayClient(
  options: LocalModelGatewayOptions = {}
): PreviewModelGatewayClient {
  // Explicit API-key configuration must not silently fall back to another account.
  if (
    process.env.FORKLAUNCH_API_KEY === undefined &&
    !options.credentialProvider &&
    process.env.FORKLAUNCH_LOCAL_MODEL_RELAY === '1'
  ) {
    return createPreviewModelGatewayClient({ fetch: options.fetch });
  }
  const applicationId =
    options.applicationId ?? process.env.FORKLAUNCH_APPLICATION_ID;
  const model = options.model ?? process.env.AGENT_MODEL;
  if (applicationId && !/^[a-f0-9-]{36}$/i.test(applicationId)) {
    throw new Error(
      'FORKLAUNCH_APPLICATION_ID must be a saved application UUID.'
    );
  }
  const provider =
    options.credentialProvider ??
    (async () => {
      const args = ['model', 'credential', '--json'];
      if (applicationId) args.push('--application-id', applicationId);
      if (model) args.push('--model', model);
      try {
        const { stdout } = await execute('forklaunch', args, {
          timeout: 35000,
          maxBuffer: 65536,
          windowsHide: true
        });
        return JSON.parse(stdout) as LocalModelCredential;
      } catch {
        // execFile errors contain stdout/stderr; never propagate credentials from them.
        throw new Error(
          'Could not connect the local agent. Run forklaunch login, check app access, and use a CLI that supports model credential.'
        );
      }
    });
  const optionsFetch = options.fetch ?? fetch;
  let pending: Promise<LocalModelCredential> | undefined;
  let credential: LocalModelCredential | undefined;
  async function connection() {
    if (credential && credential.expiresAt > Date.now()) return credential;
    pending ??= provider()
      .then(validate)
      .then((value) => (credential = value))
      .finally(() => {
        pending = undefined;
      });
    return pending;
  }
  return {
    async models() {
      const c = await connection();
      return {
        models: [{ id: c.model }],
        preview: {
          expiresAt: c.expiresAt,
          maxCalls: c.maxCalls,
          maxUnits: c.maxUnits
        }
      };
    },
    chat: {
      completions: {
        async *stream(request, options = {}) {
          const c = await connection();
          if (request.model !== c.model)
            throw new ModelGatewayRequestError(
              400,
              'Choose the model authorized for this local session.'
            );
          const response = await optionsFetch(
            `${c.gatewayBaseUrl}/chat/completions`,
            {
              method: 'POST',
              redirect: 'error',
              signal: options.signal
                ? AbortSignal.any([options.signal, AbortSignal.timeout(60000)])
                : AbortSignal.timeout(60000),
              headers: {
                authorization: `Bearer ${c.capability}`,
                'content-type': 'application/json'
              },
              body: JSON.stringify({ ...request, stream: true })
            }
          );
          if (!response.ok) {
            await response.body?.cancel();
            throw new ModelGatewayRequestError(
              response.status,
              'The local model stream was refused. No automatic retry was sent.'
            );
          }
          yield* readModelStream(response);
        },
        async create(request: GatewayChatRequest) {
          const c = await connection();
          if (request.model !== c.model)
            throw new ModelGatewayRequestError(
              400,
              'Choose the model authorized for this local session.'
            );
          const response = await (options.fetch ?? fetch)(
            `${c.gatewayBaseUrl}/chat/completions`,
            {
              method: 'POST',
              redirect: 'error',
              signal: AbortSignal.timeout(60000),
              headers: {
                authorization: `Bearer ${c.capability}`,
                'content-type': 'application/json'
              },
              body: JSON.stringify({ ...request, stream: false })
            }
          );
          if (!response.ok) {
            await response.body?.cancel();
            throw new ModelGatewayRequestError(
              response.status,
              'The local model request was refused or failed. No automatic retry was sent.'
            );
          }
          return response.json();
        }
      }
    }
  };
}

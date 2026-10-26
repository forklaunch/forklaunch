/**
 * Initializes the Forklaunch SDK with HTTP methods proxied.
 *
 * @template TypedController
 * @param {string} host - The host URL for the SDK.
 * @returns {TypedController} - The SDK proxy with methods for HTTP requests.
 */
import { ResponseContentParserType } from './src/types/contentTypes.types';
import { RegistryOptions, type UniversalSdkTransportOptions } from './src/types/sdk.types';
import { UniversalSdk } from './src/universalSdk';
import type { ZodRegexExecution } from './src/core/zodRegex';

export type UniversalSdkOptions = {
  host: string;
  registryOptions: RegistryOptions;
  contentTypeParserMap?: Record<string, ResponseContentParserType>;
  /** Host RegExp execution is synchronous and has no interruptible work budget. */
  zodRegexExecution?: ZodRegexExecution;
} & UniversalSdkTransportOptions;

export const universalSdk = async <TypedController>(options: UniversalSdkOptions) => {
  const sdkInternal = await UniversalSdk.create(
    options.host,
    options.registryOptions,
    'contentTypeParserMap' in options ? options.contentTypeParserMap : undefined,
    options.zodRegexExecution,
    { fetch: options.fetch, maxSseEventBytes: options.maxSseEventBytes,
      maxMultipartResponseBytes: options.maxMultipartResponseBytes,
      headers: options.headers, beforeRequest: options.beforeRequest,
      allowPlaintextDevelopment: options.allowPlaintextDevelopment,
      serviceBaseUrls: options.serviceBaseUrls, hmac: options.hmac }
  );

  const proxyInternal = new Proxy(sdkInternal, {
    get(target, prop) {
      if (prop === 'then' || prop === 'catch' || prop === 'finally') {
        return undefined;
      }

      if (typeof prop === 'string' && sdkInternal.isPortableSdkPath(prop)) {
        return createSdkProxy([prop]);
      }

      if (prop === 'fetch') {
        return sdkInternal.executeFetchCall;
      }

      if (typeof prop === 'string' && prop in target) {
        const value = target[prop as keyof UniversalSdk];
        if (typeof value === 'function') {
          return value.bind(target);
        }
        return value;
      }

      return createSdkProxy([prop as string]);
    }
  });

  function createSdkProxy(path: string[]): unknown {
    return new Proxy(() => {}, {
      get(_target, prop) {
        if (prop === 'then' || prop === 'catch' || prop === 'finally') {
          return undefined;
        }

        if (typeof prop === 'string' && sdkInternal.isPortableSdkPath([...path, prop].join('.'))) {
          return createSdkProxy([...path, prop]);
        }

        // TODO: revisit this to export fetch as a separate SDK client, when performance is figured out
        if (prop === 'fetch') {
          return sdkInternal.executeFetchCall;
        }

        if (typeof prop === 'string' && prop in sdkInternal) {
          const value = sdkInternal[prop as keyof UniversalSdk];
          if (typeof value === 'function') {
            return value.bind(sdkInternal);
          }
          return value;
        }

        const newPath = [...path, prop as string];

        if (
          prop === Symbol.toPrimitive ||
          prop === 'valueOf' ||
          prop === 'toString'
        ) {
          return () => sdkInternal.executeSdkCall(path.join('.'));
        }

        return createSdkProxy(newPath);
      },

      apply(_target, _thisArg, args) {
        return sdkInternal.executeSdkCall(path.join('.'), ...args);
      }
    });
  }

  return proxyInternal as TypedController;
};

export { RegistryOptions };

export type { ResponseAjvFactory, ResponseSchemaContext } from './src/core/responseDialect';

export type { ZodRegexExecution } from './src/core/zodRegex';

export type { ForkLaunchSdkRequest, ForkLaunchSdkResponse, ForkLaunchErrorResponses, ForkLaunchHeaders, ForkLaunchSdkRequestMeta } from './src/portableRuntime';

export { ForkLaunchSdkHttpError, forkLaunchQueryOptions, forkLaunchMutationOptions, createHmacAuthorization, hmacSignedFetch } from './src/portableRuntime';
export type { HmacSigningOptions } from './src/portableRuntime';
export type { ForkLaunchQuerySuccess } from './src/portableRuntime';
export type { UniversalSdkTransportOptions } from './src/types/sdk.types';

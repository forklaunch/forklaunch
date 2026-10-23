import portableRequest, { hmacSignedFetch, type ForkLaunchSdkResponse } from './portableRuntime';
import { portableOperations, type PortableOperation } from './core/portableOperation';
import { registerZodRegex, type ZodRegexExecution } from './core/zodRegex';
import {
  openApiCompliantPath,
  safeParse,
  safeStringify
} from '@forklaunch/common';
import Ajv from 'ajv';
import Ajv2020 from 'ajv/dist/2020.js';
import { draft7, draft2020, ResponseAjvFactory } from './core/responseDialect';
import addFormats from 'ajv-formats';
import { OpenAPIObject } from 'openapi3-ts/oas31';
import { mapContentType } from './core/mapContentType';
import { refreshOpenApi, getRegistryOperation } from './core/openApi';
import { ResponseSchemas, schemaPointer } from './core/responseSchema';
import { registerFactoryUriFormat } from './core/uriFormat';
import { registerZodChecks } from './core/zodChecks';
import { getSdkPath } from './core/resolvePath';
import { ResponseContentParserType } from './types/contentTypes.types';
import {
  RegistryOptions,
  RequestType,
  ResponseType,
  SdkPathMap,
  UniversalSdkTransportOptions
} from './types/sdk.types';

/**
 * A class representing the Forklaunch SDK.
 */
export class UniversalSdk {
  private registryOpenApiJson: Record<string, OpenAPIObject> | undefined;
  private registryOpenApiHash: string | undefined;
  private sdkPathMap: SdkPathMap | undefined;
  private responseSchemas: ResponseSchemas;

  /**
   * A responseAjvFactory isolates referenced schemas per registry document.
   * Without it, the supplied AJV instance (including custom keywords) is reused;
   * conflicting explicit schema IDs fail rather than replacing prior schemas.
   */
  constructor(
    private host: string,
    private ajv: Ajv,
    private registryOptions: RegistryOptions,
    private contentTypeParserMap:
      Record<string, ResponseContentParserType> | undefined,
    responseAjvFactory?: ResponseAjvFactory,
    private zodRegexExecution?: ZodRegexExecution,
    private transportOptions: UniversalSdkTransportOptions = {}
  ) {
    registerZodRegex(registerZodChecks(registerFactoryUriFormat(ajv)), zodRegexExecution);
    this.responseSchemas = new ResponseSchemas(
      ajv,
      responseAjvFactory
        ? (context) => registerZodRegex(registerZodChecks(registerFactoryUriFormat(responseAjvFactory(context))), zodRegexExecution)
        : undefined
    );
  }

  /**
   * Creates an instance of UniversalSdk.
   *
   * @param {string} host - The host URL for the SDK.
   */
  static async create(
    host: string,
    registryOptions: RegistryOptions,
    contentTypeParserMap?: Record<string, ResponseContentParserType>,
    zodRegexExecution?: ZodRegexExecution,
    transportOptions: UniversalSdkTransportOptions = {}
  ) {
    const createAjv: ResponseAjvFactory = (context) => {
      if (context.dialect !== draft7 && context.dialect !== draft2020)
        throw new Error(`Unsupported owned response schema dialect ${context.dialect}`);
      const Constructor = context.dialect === draft2020 ? Ajv2020 : Ajv;
      const ajv = new Constructor({
        coerceTypes: true,
        ownProperties: true,
        allErrors: true,
        strict: false
      });
      addFormats(ajv);
      return ajv;
    };
    const sdk = new UniversalSdk(
      host,
      createAjv({ dialect: draft7, explicit: false }),
      registryOptions,
      contentTypeParserMap,
      createAjv,
      zodRegexExecution,
      transportOptions
    );
    // Load the registry eagerly when it is reachable so portable client paths
    // resolve from the start. A registry that is not up yet (services starting
    // together) is not an error here: the first call loads it, as before.
    try {
      const initial = await refreshOpenApi(host, registryOptions, undefined, transportOptions.fetch);
      if (initial.updateRequired) {
        sdk.registryOpenApiJson = initial.registryOpenApiJson;
        sdk.registryOpenApiHash = initial.registryOpenApiHash;
        sdk.sdkPathMap = initial.sdkPathMap;
      }
    } catch (error) {
      // Retried on first use. A document supplied in-process is always
      // reachable, so an invalid one is rejected now rather than on first use.
      if ('raw' in registryOptions) throw error;
    }
    return sdk;
  }

  isPortableSdkPath(path: string): boolean {
    return Object.entries(this.sdkPathMap ?? {}).some(([key,target]) =>
      (key === path || key.startsWith(`${path}.`)) &&
      target.portableOperation !== undefined);
  }

  async executeFetchCall(
    path: string,
    request?: RequestType & {
      method:
        | 'get'
        | 'post'
        | 'put'
        | 'patch'
        | 'delete'
        | 'options'
        | 'head'
        | 'trace';
    } & {
      version?: string;
    }
  ): Promise<ResponseType | ForkLaunchSdkResponse<Record<number, unknown>>> {
    if (!this.host) {
      throw new Error('Host not initialized, please run .create(..) first');
    }

    const refreshResult = await refreshOpenApi(
      this.host,
      this.registryOptions,
      this.registryOpenApiHash,
      this.transportOptions.fetch
    );

    if (refreshResult.updateRequired) {
      this.registryOpenApiJson = refreshResult.registryOpenApiJson;
      this.registryOpenApiHash = refreshResult.registryOpenApiHash;
      this.sdkPathMap = refreshResult.sdkPathMap;
    }

    return this.execute(
      path,
      request?.method ?? 'get',
      request?.version ? `v${request.version}` : 'latest',
      request
    );
  }

  async executeSdkCall(
    sdkPath: string,
    request?: RequestType
  ): Promise<ResponseType | ForkLaunchSdkResponse<Record<number, unknown>>> {
    if (!this.host) {
      throw new Error('Host not initialized, please run .create(..) first');
    }

    const refreshResult = await refreshOpenApi(
      this.host,
      this.registryOptions,
      this.registryOpenApiHash,
      this.transportOptions.fetch
    );

    if (refreshResult.updateRequired) {
      this.registryOpenApiJson = refreshResult.registryOpenApiJson;
      this.registryOpenApiHash = refreshResult.registryOpenApiHash;
      this.sdkPathMap = refreshResult.sdkPathMap;
    }

    if (this.sdkPathMap == null) {
      throw new Error(
        'Sdk path map not initialized, please run .create(..) first'
      );
    }

    const fullSdkPath = sdkPath.split('.');
    while (fullSdkPath.length > 0) {
      if (fullSdkPath.join('.') in this.sdkPathMap) {
        break;
      }
      fullSdkPath.shift();
    }
    if (fullSdkPath.length === 0) {
      throw new Error(`Sdk path not found: ${sdkPath}`);
    }
    const { method, path, version, portableOperation } =
      this.sdkPathMap?.[fullSdkPath.join('.')] || {};

    return this.execute(path, method, version, request, portableOperation);
  }

  /**
   * Executes an HTTP request.
   *
   * @param {string} route - The route path for the request.
   * @param {'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE'} method - The HTTP method.
   * @param {RequestType} [request] - The request object.
   * @returns {Promise<ResponseType | ForkLaunchSdkResponse<Record<number, unknown>>>} - The response object.
   */
  private async execute(
    path: string,
    method: string,
    version: string,
    request?: RequestType,
    selected?: PortableOperation
  ): Promise<ResponseType | ForkLaunchSdkResponse<Record<number, unknown>>> {
    const operation = getRegistryOperation(this.registryOpenApiJson?.[version], path, method);
    const descriptors = portableOperations(operation);
    if (!selected && operation?.['x-forklaunch-operations'] !== undefined) {
      throw new TypeError('Versioned portable route requires a typed SDK operation');
    }
    const descriptor = selected ?? descriptors[0];
    if (descriptor) {
      const normalize = (value: string) => value.replace(/:([^/]+)/g, '{$1}');
      if (descriptor.method.toLowerCase() !== method || normalize(descriptor.path) !== normalize(path)) {
        throw new TypeError('Portable operation does not match registry route');
      }
      return portableRequest<Record<number, unknown>>(
        { ...this.transportOptions, baseUrl: this.host, zodRegexExecution: this.zodRegexExecution },
        { ...descriptor, input: request || {} }
      );
    }
    const { params, body, query, headers } = request || {};
    let url = getSdkPath(this.host + path).replace(/([^:]\/)\/+/g, '$1');

    if (params) {
      for (const key in params) {
        const paramValue = encodeURIComponent(params[key] as string);
        url = url.replace(`:${key}`, paramValue);
        url = url.replace(`{${key}}`, paramValue);
      }
    }

    let defaultContentType = 'application/json';
    let parsedBody;
    if (body != null) {
      if (body instanceof File || body instanceof Blob) {
        defaultContentType = 'application/octet-stream';
        parsedBody = body;
      } else if ('schema' in body && body.schema != null) {
        defaultContentType = 'application/json';
        parsedBody = safeStringify(body.schema);
      } else if ('json' in body && body.json != null) {
        defaultContentType = 'application/json';
        parsedBody = safeStringify(body.json);
      } else if ('text' in body && body.text != null) {
        defaultContentType = 'text/plain';
        parsedBody = body.text;
      } else if ('file' in body && body.file != null) {
        defaultContentType = 'application/octet-stream';
        parsedBody = body.file;
      } else if ('multipartForm' in body && body.multipartForm != null) {
        defaultContentType = 'multipart/form-data';
        const formData = new FormData();
        for (const key in body.multipartForm) {
          if (Object.prototype.hasOwnProperty.call(body.multipartForm, key)) {
            const value = body.multipartForm[key];
            if (value instanceof Blob || value instanceof File) {
              formData.append(key, value);
            } else if (Array.isArray(value)) {
              for (const item of value) {
                formData.append(
                  key,
                  item instanceof Blob || item instanceof File
                    ? item
                    : safeStringify(item)
                );
              }
            } else {
              formData.append(key, safeStringify(value));
            }
          }
        }
        parsedBody = formData;
      } else if ('urlEncodedForm' in body && body.urlEncodedForm != null) {
        defaultContentType = 'application/x-www-form-urlencoded';
        parsedBody = new URLSearchParams(
          Object.entries(body.urlEncodedForm).map(([key, value]) => [
            key,
            safeStringify(value)
          ])
        );
      } else {
        parsedBody = safeStringify(body);
      }
    }

    if (query) {
      const queryString = new URLSearchParams(
        Object.entries(query).map(([key, value]) => [key, safeStringify(value)])
      ).toString();
      url += queryString ? `?${queryString}` : '';
    }

    // Registry discovery stays unsigned; API calls sign the final wire request.
    const apiFetch = this.transportOptions.hmac
      ? hmacSignedFetch({ ...this.transportOptions.hmac, fetch: this.transportOptions.fetch })
      : (this.transportOptions.fetch ?? fetch);
    const response = await apiFetch(url, {
      method: method?.toUpperCase(),
      headers: {
        ...headers,
        ...(defaultContentType != 'multipart/form-data'
          ? { 'Content-Type': body?.contentType ?? defaultContentType }
          : {})
      },
      body: parsedBody
    });

    const responseDocument = this.registryOpenApiJson?.[version];
    const responseOpenApi =
      path != null && method != null
        ? getRegistryOperation(responseDocument, openApiCompliantPath(path), method)?.responses?.[response.status]
        : null;

    if (responseOpenApi == null) {
      throw new Error(
        `Response ${response.status} not found in OpenAPI spec for ${path} with method ${method}`
      );
    }

    const contentType = (
      response.headers.get('content-type') ||
      response.headers.get('Content-Type')
    )?.split(';')[0];

    const mappedContentType = (
      contentType != null
        ? this.contentTypeParserMap != null &&
          contentType in this.contentTypeParserMap
          ? mapContentType(this.contentTypeParserMap[contentType])
          : contentType
        : 'application/json'
    ).split(';')[0];

    const responsePointer = schemaPointer(
      openApiCompliantPath(path),
      method,
      response.status,
      contentType || mappedContentType
    );
    let responseBody;

    switch (mappedContentType) {
      case 'application/octet-stream': {
        const contentDisposition = response.headers.get('content-disposition');
        let fileName: string | null = null;
        if (contentDisposition) {
          const match = /filename\*?=(?:UTF-8''|")?([^;\r\n"]+)/i.exec(
            contentDisposition
          );
          if (match) {
            fileName = decodeURIComponent(match[1].replace(/['"]/g, ''));
          }
        }
        const blob = await response.blob();
        if (fileName == null) {
          responseBody = blob;
        } else {
          responseBody = new File([blob], fileName);
        }
        break;
      }
      case 'text/event-stream': {
        const validate = this.responseSchemas.validator(
          responseDocument!,
          responsePointer,
          responseOpenApi.content?.[contentType || mappedContentType].schema
        );
        const coerce = (value: Record<string, unknown>) =>
          this.responseSchemas.coerce(
            responseDocument!,
            responsePointer,
            responseOpenApi.content?.[contentType || mappedContentType].schema,
            value
          );
        async function* streamEvents(
          reader: ReadableStreamDefaultReader<Uint8Array>
        ) {
          const decoder = new TextDecoder();
          let buffer = '';
          let lastEventId: string | undefined;
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            buffer += decoder.decode(value, { stream: true });
            let newlineIndex;
            while ((newlineIndex = buffer.indexOf('\n')) >= 0) {
              const line = buffer.slice(0, newlineIndex).trim();
              buffer = buffer.slice(newlineIndex + 1);
              if (line.startsWith('id:')) {
                lastEventId = line.slice(3).trim();
              } else if (line.startsWith('data:')) {
                const data = line.slice(5).trim();
                const json = {
                  data: safeParse(data),
                  id: lastEventId
                };
                const isValidJson = validate(json);
                if (!isValidJson) {
                  throw new Error('Response does not match OpenAPI spec');
                }
                yield coerce(json);
              }
            }
          }
          if (buffer.length > 0) {
            let id: string | undefined;
            let data: string | undefined;
            const lines = buffer.trim().split('\n');
            for (const l of lines) {
              const line = l.trim();
              if (line.startsWith('id:')) {
                id = line.slice(3).trim();
              } else if (line.startsWith('data:')) {
                data = line.slice(5).trim();
              }
            }
            if (data !== undefined) {
              const json: Record<string, unknown> = {
                data: safeParse(data),
                id: id ?? lastEventId
              };
              const isValidJson = validate(json);
              if (!isValidJson) {
                throw new Error('Response does not match OpenAPI spec');
              }
              yield coerce(json);
            }
          }
        }
        if (!response.body) {
          throw new Error('No response body for event stream');
        }
        responseBody = streamEvents(response.body.getReader());
        break;
      }
      case 'text/plain':
        responseBody = await response.text();
        break;
      case 'application/json':
      default: {
        const json: Record<string, unknown> = await response.json();
        const validate = this.responseSchemas.validator(
          responseDocument!,
          responsePointer,
          responseOpenApi.content?.[contentType || mappedContentType].schema
        );
        const isValidJson = validate(json);
        if (!isValidJson) {
          throw new Error(
            `Response does not match OpenAPI spec: ${JSON.stringify(validate.errors)}`
          );
        }
        responseBody = this.responseSchemas.coerce(
          responseDocument!,
          responsePointer,
          responseOpenApi.content?.[contentType || mappedContentType].schema,
          json
        );
        break;
      }
    }

    return {
      code: response.status,
      response: responseBody,
      headers: response.headers
    };
  }
}

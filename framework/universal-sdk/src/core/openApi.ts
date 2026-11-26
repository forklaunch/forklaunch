import { portableOperations } from './portableOperation';
import { OpenAPIObject, OperationObject } from 'openapi3-ts/oas31';
import { isOpenAPIObject } from '../guards/isOpenApiObject';
import { RegistryOptions, SdkPathMap } from '../types/sdk.types';

export function getSdkPathMap(
  registryOpenApiJson: Record<string, OpenAPIObject>
): SdkPathMap {
  const sdkPathMap: SdkPathMap = {};
  Object.entries(registryOpenApiJson).forEach(([version, openApi]) => {
    Object.entries(openApi?.paths || {}).forEach(([path, pathItem]) => {
      const methods = [
        'get',
        'post',
        'put',
        'patch',
        'delete',
        'options',
        'head',
        'trace'
      ] as const;

      const versionedPath =
        version === 'latest' ? undefined : version.substring(1);
      const extended = pathItem['x-forklaunch-http-methods'];
      if (extended !== undefined && (extended === null || typeof extended !== 'object' || Array.isArray(extended))) {
        throw new TypeError('Invalid extended HTTP operation map');
      }
      const entries: Array<[string, OperationObject | undefined]> = methods.map(method => [method, pathItem[method]]);
      for (const [method, operation] of Object.entries(extended ?? {})) {
        if (!/^[A-Z][A-Z0-9-]*$/.test(method) || methods.some(standard => standard === method.toLowerCase()) ||
            operation === null || typeof operation !== 'object' || Array.isArray(operation)) {
          throw new TypeError('Invalid extended HTTP operation');
        }
        if (!portableOperations(operation).length) throw new TypeError('Extended HTTP operation requires a checked descriptor');
        entries.push([method.toLowerCase(), operation as OperationObject]);
      }
      entries.forEach(([method, operation]) => {
        const descriptors = portableOperations(operation);
        for (const portable of descriptors) {
          const key = portable.clientPath.join('.');
          if (Object.hasOwn(sdkPathMap, key)) throw new TypeError('Duplicate portable SDK client path');
          sdkPathMap[key] = { method, path, version, portableOperation: portable };
        }
        // A plural route must be addressed through a typed projection. Its base
        // operation ID has no single request/response contract to execute.
        if (operation?.operationId && operation['x-forklaunch-operations'] === undefined) {
          const key = `${operation.operationId}${versionedPath ? `.${versionedPath}` : ''}`;
          // A repeated operation ID resolves to the last route that declares it,
          // as it always has; rejecting it would break existing registries.
          sdkPathMap[key] = { method, path, version, portableOperation: descriptors[0] };
        }
      });
    });
  });

  return sdkPathMap;
}

export function getRegistryOperation(openApi: OpenAPIObject | undefined, path: string, method: string): OperationObject | undefined {
  const pathItem = openApi?.paths?.[path];
  if (!pathItem) return undefined;
  const entry = (pathItem as Record<string, unknown>)[method.toLowerCase()] ??
    pathItem['x-forklaunch-http-methods']?.[method.toUpperCase()];
  return entry && typeof entry === 'object' && !Array.isArray(entry) ? entry as OperationObject : undefined;
}

export async function refreshOpenApi(
  host: string,
  registryOptions: RegistryOptions,
  existingRegistryOpenApiHash?: string,
  fetcher: typeof fetch = fetch
): Promise<
  | {
      updateRequired: true;
      registryOpenApiJson: Record<string, OpenAPIObject>;
      registryOpenApiHash: string;
      sdkPathMap: SdkPathMap;
    }
  | {
      updateRequired: false;
    }
> {
  if (
    existingRegistryOpenApiHash === 'static' ||
    ('static' in registryOptions && registryOptions.static)
  ) {
    return {
      updateRequired: false
    };
  }

  if ('raw' in registryOptions) {
    const rawVersionedOpenApi = isOpenAPIObject(registryOptions.raw)
      ? {
          latest: registryOptions.raw
        }
      : registryOptions.raw;
    return {
      updateRequired: true,
      registryOpenApiJson: rawVersionedOpenApi,
      registryOpenApiHash: 'static',
      sdkPathMap: getSdkPathMap(rawVersionedOpenApi)
    };
  }

  const registry =
    'path' in registryOptions
      ? `${host}/${registryOptions.path}`
      : 'url' in registryOptions
        ? registryOptions.url
        : null;

  if (registry == null) {
    throw new Error('Raw OpenAPI JSON or registry information not provided');
  }

  const registryOpenApiHashFetch = await fetcher(`${registry}-hash`);
  if (!registryOpenApiHashFetch.ok) {
    throw new Error(
      `Failed to fetch OpenAPI registry hash: ${registryOpenApiHashFetch.status} ${registryOpenApiHashFetch.statusText}`
    );
  }
  const registryOpenApiHash = await registryOpenApiHashFetch.text();

  if (
    existingRegistryOpenApiHash == null ||
    existingRegistryOpenApiHash !== registryOpenApiHash
  ) {
    const registryOpenApiFetch = await fetcher(registry);
    if (!registryOpenApiFetch.ok) {
      throw new Error(
        `Failed to fetch OpenAPI registry: ${registryOpenApiFetch.status} ${registryOpenApiFetch.statusText}`
      );
    }
    const fetchedRegistry = await registryOpenApiFetch.json();
    const registryOpenApiJson = isOpenAPIObject(fetchedRegistry) ? {latest: fetchedRegistry} : fetchedRegistry;

    return {
      updateRequired: true,
      registryOpenApiJson,
      registryOpenApiHash,
      sdkPathMap: getSdkPathMap(registryOpenApiJson)
    };
  }

  return {
    updateRequired: false
  };
}

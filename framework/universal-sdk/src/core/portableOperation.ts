import type { ForkLaunchSdkRequestMeta } from '../portableRuntime';

export type PortableOperation = Omit<ForkLaunchSdkRequestMeta, 'input'> & {
  format: 'forklaunch.sdk-operation.v1';
  clientPath: string[];
};

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** Reject malformed extensions; never silently fall back to a weaker decoder. */
export function portableOperation(value: unknown): PortableOperation {
  if (!record(value) || value.format !== 'forklaunch.sdk-operation.v1') {
    throw new TypeError('Unsupported portable SDK operation format');
  }
  for (const key of ['service', 'router', 'operation', 'method', 'path', 'requestKind', 'requestContentType']) {
    if (typeof value[key] !== 'string') throw new TypeError('Invalid portable operation metadata');
  }
  if (!Array.isArray(value.clientPath) || !value.clientPath.length ||
      value.clientPath.some(p => typeof p !== 'string' || !/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(p) || ['__proto__','constructor','prototype','then','catch','finally'].includes(p))) {
    throw new TypeError('Invalid portable SDK client path');
  }
  if (!['json','text','binary','urlEncoded','multipart'].includes(String(value.requestKind)) ||
      !('requestSchema' in value) || !(record(value.requestSchema) || typeof value.requestSchema === 'boolean')) {
    throw new TypeError('Invalid portable request contract');
  }
  for (const key of ['responseKinds', 'responseSchemas', 'responseHeaders']) {
    if (!record(value[key])) throw new TypeError('Missing portable response contract');
  }
  const kinds = value.responseKinds as Record<string, unknown>;
  const schemas = value.responseSchemas as Record<string, unknown>;
  for (const [key, kind] of Object.entries(kinds)) {
    if (!/^\d{3}:/.test(key) || !['empty','json','text','binary','sse','multipart'].includes(String(kind)) ||
        !Object.prototype.hasOwnProperty.call(schemas,key) || !(record(schemas[key]) || typeof schemas[key] === 'boolean')) {
      throw new TypeError('Invalid portable response contract');
    }
  }
  if (Object.keys(schemas).some(key => !(key in kinds))) throw new TypeError('Inconsistent portable response contract');
  // All wire data is cloned; registry refresh cannot mutate an in-flight operation.
  return structuredClone(value) as PortableOperation;
}

/** A versioned HTTP route has one checked descriptor per typed SDK operation. */
export function portableOperations(operation: unknown): PortableOperation[] {
  if (!record(operation)) return [];
  const singular = operation['x-forklaunch-operation'];
  const plural = operation['x-forklaunch-operations'];
  if (singular !== undefined && plural !== undefined) {
    throw new TypeError('Conflicting portable SDK operation extensions');
  }
  if (plural !== undefined) {
    if (!Array.isArray(plural) || plural.length === 0) {
      throw new TypeError('Portable SDK operations must be a nonempty array');
    }
    return plural.map(portableOperation);
  }
  return singular === undefined ? [] : [portableOperation(singular)];
}

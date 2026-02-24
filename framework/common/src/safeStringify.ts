/**
 * Safely stringifies any JavaScript value, handling special cases like:
 * - Error objects
 * - BigInt
 * - Functions
 * - Symbols
 * - Special number values (NaN, Infinity)
 * - Built-in objects (Date, RegExp)
 * - Collections (Map, Set)
 * - TypedArrays and ArrayBuffers
 *
 * @param arg - The value to stringify
 * @returns A string representation of the value
 *
 * @example
 * ```typescript
 * // Handle Error objects
 * safeStringify(new Error("test")); // '{"name":"Error","message":"test","stack":"..."}'
 *
 * // Handle special types
 * safeStringify(BigInt(123)); // "123n"
 * safeStringify(Symbol("test")); // "Symbol(test)"
 * safeStringify(() => {}); // "[Function: anonymous]"
 * safeStringify(new Map([["key", "value"]])); // '{"__type":"Map","value":[["key","value"]]}'
 * ```
 */
export function safeStringify(arg: unknown): string {
  if (typeof arg === 'string') {
    return arg;
  }

  if (arg == null) {
    return String(arg);
  }

  const replacer = (key: string, value: unknown): unknown => {
    if (value instanceof Error) {
      return {
        name: value.name,
        message: value.message,
        stack: value.stack,
        cause: value.cause
      };
    }

    if (typeof value === 'bigint') {
      return value.toString() + 'n';
    }

    if (typeof value === 'function') {
      return `[Function: ${value.name || 'anonymous'}]`;
    }

    if (typeof value === 'symbol') {
      return value.toString();
    }

    if (typeof value === 'number') {
      if (Number.isNaN(value)) return 'NaN';
      if (value === Infinity) return 'Infinity';
      if (value === -Infinity) return '-Infinity';
    }

    if (value instanceof Date) {
      return value.toISOString();
    }

    if (value instanceof RegExp) {
      return value.toString();
    }

    if (value instanceof Map) {
      return {
        __type: 'Map',
        value: Array.from(value.entries())
      };
    }

    if (value instanceof Set) {
      return {
        __type: 'Set',
        value: Array.from(value.values())
      };
    }

    if (ArrayBuffer.isView(value)) {
      return {
        __type: value.constructor.name,
        value: Array.from(value as unknown as ArrayLike<unknown>)
      };
    }

    if (value instanceof ArrayBuffer) {
      return {
        __type: 'ArrayBuffer',
        value: Array.from(new Uint8Array(value))
      };
    }

    return value;
  };

  try {
    return JSON.stringify(arg, replacer);
  } catch (error: unknown) {
    if (error instanceof Error) {
      return `[Unserializable: ${error.message}]`;
    }
    return '[Unserializable: Unknown error]';
  }
}

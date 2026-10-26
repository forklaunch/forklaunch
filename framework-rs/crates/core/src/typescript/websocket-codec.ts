// Transport admission shared by every generated Node and browser socket client.
function socketTransportUrl(value: string): URL {
  const endpoint = new URL(value);
  const loopback = endpoint.hostname === 'localhost' || endpoint.hostname === '[::1]'
    || /^127(?:\.\d{1,3}){3}$/.test(endpoint.hostname);
  if (endpoint.username || endpoint.password || endpoint.hash
    || !(endpoint.protocol === 'wss:' || (endpoint.protocol === 'ws:' && loopback))) {
    throw new Error('WebSocket TLS is required outside loopback development');
  }
  return endpoint;
}
// End transport admission.

function socketInteger(
  value: number | undefined,
  fallback: number,
  label: string,
): number {
  const result = value ?? fallback;
  if (!Number.isSafeInteger(result) || result <= 0)
    throw new Error(`Invalid ${label}`);
  return result;
}
function socketAbort(): Error {
  const error = new Error('WebSocket operation aborted');
  error.name = 'AbortError';
  return error;
}
function socketTimeout(
  value: number | undefined,
  fallback: number,
  label: string,
): number {
  const result = socketInteger(value, fallback, label);
  if (result > 2_147_483_647)
    throw new Error(`Invalid ${label}: exceeds timer range`);
  return result;
}
function socketError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}
function socketJsonBounds(value: unknown, byteLimit: number): void {
  const pending: Array<[unknown, number, boolean?]> = [[value, 0]];
  const seen = new Set<object>();
  // A valid bounded wire document cannot contain more nodes than its bytes.
  let nodes = byteLimit + 1;
  while (pending.length) {
    const [current, depth, leaving] = pending.pop()!;
    if (leaving) {
      seen.delete(current as object);
      continue;
    }
    if (--nodes < 0 || depth >= 128)
      throw new Error('WebSocket JSON representation limit');
    if (typeof current === 'number' && !Number.isFinite(current))
      throw new Error('Non-finite WebSocket JSON number');
    if (typeof current === 'string') {
      if (current.length > byteLimit || new TextEncoder().encode(current).byteLength > byteLimit)
        throw new Error('WebSocket payload byte limit');
      for (let index = 0; index < current.length; index++) {
        const code = current.charCodeAt(index);
        if (code >= 0xd800 && code <= 0xdbff) {
          const next = current.charCodeAt(++index);
          if (!(next >= 0xdc00 && next <= 0xdfff))
            throw new Error('Unpaired WebSocket JSON surrogate');
        } else if (code >= 0xdc00 && code <= 0xdfff)
          throw new Error('Unpaired WebSocket JSON surrogate');
      }
    }
    if (current && typeof current === 'object') {
      if (seen.has(current)) throw new Error('Circular WebSocket JSON object');
      seen.add(current);
      pending.push([current, depth, true]);
      const values = Array.isArray(current) ? current : Object.values(current);
      if (values.length > nodes - pending.length)
        throw new Error('WebSocket JSON node limit');
      for (const child of values) pending.push([child, depth + 1]);
    }
  }
}
class PortableSocketCodec<T> {
  private readonly validate?: ValidateFunction;
  constructor(
    private readonly schema: AnySchema,
    private readonly limit: number,
    private readonly binary = false,
    private readonly jsonEncoding = false,
    regexExecution?: ZodRegexExecution,
  ) {
    if (!binary) this.validate = socketValidator(schema, regexExecution);
  }
  private check(value: unknown): T {
    socketJsonBounds(value, this.limit);
    if (!this.validate?.(value))
      throw new Error(
        `Invalid WebSocket payload: ${JSON.stringify(this.validate?.errors)}`,
      );
    return value as T;
  }
  decode(bytes: Uint8Array): T {
    if (bytes.length > this.limit)
      throw new Error('WebSocket payload byte limit');
    if (this.binary) return Uint8Array.from(bytes) as T;
    const text = this.jsonEncoding ? new TextDecoder('utf-8', {fatal:true, ignoreBOM:true}).decode(bytes) : new TextDecoder('utf-8', {ignoreBOM:true}).decode(bytes);
    let value: unknown;
    try {
      value = JSON.parse(text);
    } catch {
      if (this.jsonEncoding) throw new Error('Invalid JSON WebSocket payload');
      value = text;
    }
    return portableDates(this.schema, this.check(value)) as T;
  }
  encode(value: T): Uint8Array {
    if (this.binary) {
      if (!(value instanceof Uint8Array)) throw new Error('Expected binary WebSocket Uint8Array');
      if (value.byteLength > this.limit) throw new Error('WebSocket payload byte limit');
      return Uint8Array.from(value);
    }
    // Validate the serialized JSON projection, matching serde's outgoing values.
    const wire = portableDates(this.schema, value, true);
    socketJsonBounds(wire, this.limit);
    const json = JSON.stringify(wire);
    if (json === undefined || (!this.jsonEncoding && value === null))
      throw new Error('Null/undefined WebSocket messages are unsupported');
    if (new TextEncoder().encode(json).byteLength > this.limit * 6 + 2)
      throw new Error('WebSocket payload byte limit');
    const projected: unknown = JSON.parse(json);
    portableDates(this.schema, this.check(projected));
    const bytes = new TextEncoder().encode(!this.jsonEncoding && typeof projected === 'string' ? projected : json);
    if (bytes.length > this.limit)
      throw new Error('WebSocket payload byte limit');
    return bytes;
  }
}

import { describe, expect, it } from 'vitest';
import {
  enrichResponseTransmission,
  headerValue
} from '../src/middleware/enrichResponseTransmission.middleware';

// Runs the middleware on a bare response and records what reaches
// hyper-express's own setHeader.
function wrappedSetHeader() {
  const written: [string, unknown][] = [];
  const res = {
    send: () => undefined,
    json: () => undefined,
    setHeader(name: string, value: unknown) {
      written.push([name, value]);
      return res;
    }
  };
  enrichResponseTransmission(
    {} as never,
    res as never,
    (() => undefined) as never
  );
  return { setHeader: res.setHeader.bind(res), written };
}

describe('hyper-express setHeader', () => {
  it('sends strings unquoted and other values as plain strings', () => {
    const { setHeader, written } = wrappedSetHeader();
    setHeader('content-type', 'text/plain');
    setHeader('x-count', 3);
    setHeader('x-meta', { a: 1 });
    expect(written).toEqual([
      ['content-type', 'text/plain'],
      ['x-count', '3'],
      ['x-meta', '{"a":1}']
    ]);
  });

  it('passes multiple values through, one header line each', () => {
    const { setHeader, written } = wrappedSetHeader();
    setHeader('set-cookie', ['a=1', 'b=2']);
    expect(written).toEqual([['set-cookie', ['a=1', 'b=2']]]);
  });

  it('refuses CR, LF and NUL, so a value cannot add headers', () => {
    const { setHeader, written } = wrappedSetHeader();
    for (const value of [
      'ok\r\nset-cookie: admin=1',
      'ok\nx-injected: 1',
      'ok\0',
      ['fine', 'bad\r\nx-injected: 1']
    ]) {
      expect(() => setHeader('x-user', value)).toThrow(TypeError);
    }
    expect(written).toEqual([]);
  });

  it('refuses an unserializable value, whose message spans lines', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(() => headerValue('x-circular', circular)).toThrow(
      'Invalid character in header content ["x-circular"]'
    );
  });
});

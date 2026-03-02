import { describe, expect, it } from 'vitest';
import { safeParse } from '../src/safeParse';
import { safeStringify, toPlainString } from '../src/safeStringify';

describe('safeStringify', () => {
  it('should handle primitive types', () => {
    expect(safeStringify('test')).toBe('"test"');
    expect(safeStringify(123)).toBe('123');
    expect(safeStringify(true)).toBe('true');
    expect(safeStringify(null)).toBe('null');
    expect(safeStringify(undefined)).toBe('undefined');
  });

  it('should properly JSON-encode strings to prevent type confusion', () => {
    // Strings should be JSON-encoded with quotes
    expect(safeStringify('hello')).toBe('"hello"');
    expect(safeStringify('123')).toBe('"123"');
    expect(safeStringify('')).toBe('""');

    // Type safety: string "123" should differ from number 123
    expect(safeStringify('123')).not.toBe(safeStringify(123));
    expect(safeStringify('123')).toBe('"123"');  // String with quotes
    expect(safeStringify(123)).toBe('123');      // Number without quotes

    // Special characters should be escaped
    expect(safeStringify('hello "world"')).toBe('"hello \\"world\\""');
    expect(safeStringify('line1\nline2')).toBe('"line1\\nline2"');

    // Verify JSON semantics are preserved
    expect(safeStringify('test')).toBe(JSON.stringify('test'));
    expect(safeStringify('hello world')).toBe(JSON.stringify('hello world'));
  });

  it('should handle Error objects', () => {
    const error = new Error('test error');
    const result = safeParse<typeof error>(safeStringify(error));
    expect(result.name).toBe('Error');
    expect(result.message).toBe('test error');
    expect(result.stack).toBeDefined();
  });

  it('should handle special types', () => {
    expect(safeStringify(BigInt(123))).toBe('"123n"');
    expect(safeStringify(Symbol('test'))).toBe('"Symbol(test)"');
    expect(safeStringify(() => { })).toBe('"[Function: anonymous]"');
    expect(safeStringify(function named() { })).toBe('"[Function: named]"');
  });

  it('should handle special number values', () => {
    expect(safeStringify(NaN)).toBe('"NaN"');
    expect(safeStringify(Infinity)).toBe('"Infinity"');
    expect(safeStringify(-Infinity)).toBe('"-Infinity"');
  });

  it('should handle built-in objects', () => {
    const date = new Date('2023-01-01');
    expect(safeStringify(date)).toBe('"2023-01-01T00:00:00.000Z"');
    expect(safeStringify(/test/g)).toBe('"/test/g"');
  });

  it('should handle collections', () => {
    const map = new Map([['key', 'value']]);
    const set = new Set([1, 2, 3]);

    expect(safeParse(safeStringify(map))).toEqual({
      __type: 'Map',
      value: [['key', 'value']]
    });

    expect(safeParse(safeStringify(set))).toEqual({
      __type: 'Set',
      value: [1, 2, 3]
    });
  });

  it('should handle typed arrays', () => {
    const uint8Array = new Uint8Array([1, 2, 3]);
    expect(safeParse(safeStringify(uint8Array))).toEqual({
      __type: 'Uint8Array',
      value: [1, 2, 3]
    });
  });

  it('should handle nested complex objects', () => {
    interface CircularRef {
      circular: unknown;
    }

    const complex = {
      error: new Error('test'),
      fn: () => { },
      sym: Symbol('test'),
      date: new Date('2023-01-01'),
      map: new Map([['key', 'value']]),
      nested: {
        array: [1, 2, { circular: null } as CircularRef]
      }
    };

    const result = safeParse<typeof complex>(safeStringify(complex));
    expect(result.error.message).toBe('test');
    expect(result.fn).toBe('[Function: fn]');
    expect(result.sym).toBe('Symbol(test)');
    expect(result.date).toBe('2023-01-01T00:00:00.000Z');
  });

  it('should handle unserializable objects', () => {
    const unserializable = {
      toJSON() {
        throw new Error('Cannot serialize');
      }
    };
    expect(safeStringify(unserializable)).toBe(
      '[Unserializable: Cannot serialize]'
    );

    const unknownError = {
      toJSON() {
        throw 'Unknown error';
      }
    };
    expect(safeStringify(unknownError)).toBe('[Unserializable: Unknown error]');
  });
});

describe('toPlainString', () => {
  it('should convert primitives to plain strings without JSON encoding', () => {
    // Strings should NOT be JSON-encoded (no extra quotes)
    expect(toPlainString('hello')).toBe('hello');
    expect(toPlainString('test')).toBe('test');
    expect(toPlainString('')).toBe('');

    // Numbers and booleans
    expect(toPlainString(123)).toBe('123');
    expect(toPlainString(0)).toBe('0');
    expect(toPlainString(true)).toBe('true');
    expect(toPlainString(false)).toBe('false');

    // null and undefined
    expect(toPlainString(null)).toBe('null');
    expect(toPlainString(undefined)).toBe('undefined');
  });

  it('should differ from safeStringify for strings', () => {
    // This is the key difference - toPlainString does NOT JSON-encode strings
    expect(toPlainString('hello')).toBe('hello');
    expect(safeStringify('hello')).toBe('"hello"');

    expect(toPlainString('123')).toBe('123');
    expect(safeStringify('123')).toBe('"123"');
  });

  it('should JSON-stringify objects and arrays', () => {
    // Objects and arrays should be JSON-stringified
    expect(toPlainString({ a: 1 })).toBe('{"a":1}');
    expect(toPlainString([1, 2, 3])).toBe('[1,2,3]');
    expect(toPlainString({ key: 'value' })).toBe('{"key":"value"}');
  });

  it('should be suitable for form data and query parameters', () => {
    // These are the use cases for toPlainString
    const formData = new FormData();
    formData.append('name', toPlainString('John Doe'));
    formData.append('age', toPlainString(30));
    formData.append('active', toPlainString(true));

    expect(formData.get('name')).toBe('John Doe');
    expect(formData.get('age')).toBe('30');
    expect(formData.get('active')).toBe('true');

    // Query parameters
    const params = new URLSearchParams();
    params.append('q', toPlainString('search term'));
    params.append('page', toPlainString(1));
    params.append('filter', toPlainString({ status: 'active' }));

    expect(params.get('q')).toBe('search term');
    expect(params.get('page')).toBe('1');
    expect(params.get('filter')).toBe('{"status":"active"}');
  });

  it('should handle unserializable values safely', () => {
    // Circular reference
    interface CircularObject {
      self?: CircularObject;
    }
    const circular: CircularObject = {};
    circular.self = circular;

    const circularResult = toPlainString(circular);
    expect(circularResult).toMatch(/\[Unserializable:/);

    // BigInt
    const bigIntValue = BigInt(123);
    const bigIntResult = toPlainString(bigIntValue);
    expect(bigIntResult).toMatch(/\[Unserializable:/);

    // Object with toJSON that throws
    const throwingObject = {
      toJSON() {
        throw new Error('Cannot serialize');
      }
    };
    expect(toPlainString(throwingObject)).toBe('[Unserializable: Cannot serialize]');
  });
});

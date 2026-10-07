import {
  any,
  bigint,
  binary,
  boolean,
  date,
  email,
  file,
  never,
  null_,
  nullish,
  number,
  SchemaValidator,
  string,
  symbol,
  type,
  undefined_,
  unknown,
  uri,
  uuid,
  void_
} from '../../src/zod';
import { compare } from '../utils/compare';

describe('zod advanced parse', () => {
  const schemaValidator = SchemaValidator();

  describe('string', () => {
    test('valid string', () => {
      compare(schemaValidator.parse(string, 'hello'), {
        ok: true,
        value: 'hello'
      });
    });

    test('invalid string with number', () => {
      expect(schemaValidator.parse(string, 123).ok).toBe(false);
    });

    test('invalid string with boolean', () => {
      expect(schemaValidator.parse(string, true).ok).toBe(false);
    });

    test('invalid string with object', () => {
      expect(schemaValidator.parse(string, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid string with array', () => {
      expect(schemaValidator.parse(string, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid string with null', () => {
      expect(schemaValidator.parse(string, null).ok).toBe(false);
    });

    test('invalid string with undefined', () => {
      expect(schemaValidator.parse(string, undefined).ok).toBe(false);
    });

    test('invalid string with symbol', () => {
      expect(schemaValidator.parse(string, Symbol('hello')).ok).toBe(false);
    });

    test('invalid string with bigint', () => {
      expect(schemaValidator.parse(string, BigInt(123)).ok).toBe(false);
    });

    test('invalid string with date', () => {
      expect(schemaValidator.parse(string, new Date()).ok).toBe(false);
    });
  });

  describe('uuid', () => {
    test('valid uuid', () => {
      compare(
        schemaValidator.parse(uuid, 'f651c6c5-f628-48de-904c-f37509a97ab6'),
        {
          ok: true,
          value: 'f651c6c5-f628-48de-904c-f37509a97ab6'
        }
      );
    });

    test('invalid uuid', () => {
      expect(schemaValidator.parse(uuid, 'hello').ok).toBe(false);
    });
  });

  describe('uri', () => {
    test('valid uri', () => {
      compare(schemaValidator.parse(uri, 'https://forklaunch.com'), {
        ok: true,
        value: 'https://forklaunch.com'
      });
    });

    test('invalid uri', () => {
      expect(schemaValidator.parse(uri, 'hello').ok).toBe(false);
    });
  });

  describe('email', () => {
    test('valid email', () => {
      compare(schemaValidator.parse(email, 'test@forklaunch.com'), {
        ok: true,
        value: 'test@forklaunch.com'
      });
    });

    test('invalid email', () => {
      expect(schemaValidator.parse(email, 'hello').ok).toBe(false);
    });
  });

  describe('number', () => {
    test('valid number', () => {
      compare(schemaValidator.parse(number, 123), {
        ok: true,
        value: 123
      });
    });

    test('valid number with numeric string', () => {
      compare(schemaValidator.parse(number, 123), {
        ok: true,
        value: 123
      });
    });

    test('valid number with boolean', () => {
      compare(schemaValidator.parse(number, true), {
        ok: true,
        value: 1
      });
    });

    test('valid number with null', () => {
      compare(schemaValidator.parse(number, null), {
        ok: true,
        value: 0
      });
    });

    test('valid number with date', () => {
      compare(schemaValidator.parse(number, new Date(0)), {
        ok: true,
        value: 0
      });
    });

    test('valid number with bigint', () => {
      compare(schemaValidator.parse(number, BigInt(123)), {
        ok: true,
        value: 123
      });
    });

    test('invalid number with string', () => {
      expect(schemaValidator.parse(number, 'hello').ok).toBe(false);
    });

    test('invalid number with object', () => {
      expect(schemaValidator.parse(number, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid number with array', () => {
      expect(schemaValidator.parse(number, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid number with undefined', () => {
      expect(schemaValidator.parse(number, undefined).ok).toBe(false);
    });

    test('invalid number with symbol', () => {
      expect(schemaValidator.parse(number, Symbol('hello')).ok).toBe(false);
    });
  });

  describe('bigint', () => {
    test('valid bigint', () => {
      const parsed = schemaValidator.parse(
        bigint,
        BigInt('12345678901234567890')
      );
      if (parsed.ok) {
        compare(parsed.value, BigInt('12345678901234567890'));
      } else {
        expect.fail('Expected parsed.ok to be true');
      }
    });

    test('valid bigint with number', () => {
      const parsed = schemaValidator.parse(bigint, 123123);
      if (parsed.ok) {
        compare(parsed.value, BigInt(123123));
      } else {
        expect.fail('Expected parsed.ok to be true');
      }
    });

    test('valid bigint with boolean', () => {
      const parsed = schemaValidator.parse(bigint, true);
      if (parsed.ok) {
        compare(parsed.value, BigInt(1));
      } else {
        expect.fail('Expected parsed.ok to be true');
      }
    });

    test('valid bigint with date', () => {
      const parsed = schemaValidator.parse(bigint, new Date(0));
      if (parsed.ok) {
        compare(parsed.value, BigInt(0));
      } else {
        expect.fail('Expected parsed.ok to be true');
      }
    });

    test('invalid bigint with string', () => {
      expect(schemaValidator.parse(bigint, 'hello').ok).toBe(false);
    });

    test('invalid bigint with object', () => {
      expect(schemaValidator.parse(bigint, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid bigint with array', () => {
      expect(schemaValidator.parse(bigint, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid bigint with null', () => {
      expect(schemaValidator.parse(bigint, null).ok).toBe(false);
    });

    test('invalid bigint with undefined', () => {
      expect(schemaValidator.parse(bigint, undefined).ok).toBe(false);
    });

    test('invalid bigint with symbol', () => {
      expect(schemaValidator.parse(bigint, Symbol('hello')).ok).toBe(false);
    });
  });

  describe('boolean', () => {
    test('valid boolean', () => {
      compare(schemaValidator.parse(boolean, true), {
        ok: true,
        value: true
      });
    });

    test('valid boolean with boolean string', () => {
      compare(schemaValidator.parse(boolean, 'true'), {
        ok: true,
        value: true
      });
    });

    test('invalid boolean with string', () => {
      expect(schemaValidator.parse(boolean, 'hello').ok).toBe(false);
    });

    test('invalid boolean with number', () => {
      expect(schemaValidator.parse(boolean, 123).ok).toBe(false);
    });

    test('invalid boolean with object', () => {
      expect(schemaValidator.parse(boolean, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid boolean with array', () => {
      expect(schemaValidator.parse(boolean, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid boolean with null', () => {
      expect(schemaValidator.parse(boolean, null).ok).toBe(false);
    });

    test('invalid boolean with undefined', () => {
      expect(schemaValidator.parse(boolean, undefined).ok).toBe(false);
    });

    test('invalid boolean with symbol', () => {
      expect(schemaValidator.parse(boolean, Symbol('hello')).ok).toBe(false);
    });

    test('invalid boolean with bigint', () => {
      expect(schemaValidator.parse(boolean, BigInt(123)).ok).toBe(false);
    });

    test('invalid boolean with date', () => {
      expect(schemaValidator.parse(boolean, new Date()).ok).toBe(false);
    });
  });

  describe('date', () => {
    test('valid date', () => {
      const dateTest = new Date();
      compare(schemaValidator.parse(date, dateTest), {
        ok: true,
        value: dateTest
      });
    });

    test('valid date with datestring', () => {
      compare(schemaValidator.parse(date, '2021-01-01'), {
        ok: true,
        value: new Date('2021-01-01')
      });
    });

    test('valid date with number', () => {
      compare(schemaValidator.parse(date, 123), {
        ok: true,
        value: new Date(123)
      });
    });

    test('invalid date with boolean', () => {
      expect(schemaValidator.parse(date, true).ok).toBe(false);
    });

    test('invalid date with null', () => {
      expect(schemaValidator.parse(date, null).ok).toBe(false);
    });

    test('invalid date with string', () => {
      expect(schemaValidator.parse(date, 'hello').ok).toBe(false);
    });

    test('invalid date with object', () => {
      expect(schemaValidator.parse(date, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid date with array', () => {
      expect(schemaValidator.parse(date, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid date with undefined', () => {
      expect(schemaValidator.parse(date, undefined).ok).toBe(false);
    });

    test('invalid date with symbol', () => {
      expect(schemaValidator.parse(date, Symbol('hello')).ok).toBe(false);
    });

    test('invalid date with bigint', () => {
      expect(schemaValidator.parse(date, BigInt(123)).ok).toBe(false);
    });
  });

  describe('symbol', () => {
    test('valid symbol', () => {
      const symbolTest = Symbol('hello');
      compare(schemaValidator.parse(symbol, symbolTest), {
        ok: true,
        value: symbolTest
      });
    });

    test('invalid symbol with string', () => {
      expect(schemaValidator.parse(symbol, 'hello').ok).toBe(false);
    });

    test('invalid symbol with number', () => {
      expect(schemaValidator.parse(symbol, 123).ok).toBe(false);
    });

    test('invalid symbol with boolean', () => {
      expect(schemaValidator.parse(symbol, true).ok).toBe(false);
    });

    test('invalid symbol with object', () => {
      expect(schemaValidator.parse(symbol, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid symbol with array', () => {
      expect(schemaValidator.parse(symbol, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid symbol with null', () => {
      expect(schemaValidator.parse(symbol, null).ok).toBe(false);
    });

    test('invalid symbol with undefined', () => {
      expect(schemaValidator.parse(symbol, undefined).ok).toBe(false);
    });

    test('invalid symbol with bigint', () => {
      expect(schemaValidator.parse(symbol, BigInt(123)).ok).toBe(false);
    });

    test('invalid symbol with date', () => {
      expect(schemaValidator.parse(symbol, new Date()).ok).toBe(false);
    });
  });

  describe('nullish', () => {
    test('valid nullish with null', () => {
      compare(schemaValidator.parse(nullish, null), {
        ok: true,
        value: null
      });
    });

    test('valid nullish with undefined', () => {
      compare(schemaValidator.parse(nullish, undefined), {
        ok: true,
        value: undefined
      });
    });

    test('invalid nullish with string', () => {
      expect(schemaValidator.parse(nullish, 'hello').ok).toBe(false);
    });

    test('invalid nullish with number', () => {
      expect(schemaValidator.parse(nullish, 123).ok).toBe(false);
    });

    test('invalid nullish with boolean', () => {
      expect(schemaValidator.parse(nullish, true).ok).toBe(false);
    });

    test('invalid nullish with object', () => {
      expect(schemaValidator.parse(nullish, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid nullish with array', () => {
      expect(schemaValidator.parse(nullish, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid nullish with symbol', () => {
      expect(schemaValidator.parse(nullish, Symbol('hello')).ok).toBe(false);
    });

    test('invalid nullish with bigint', () => {
      expect(schemaValidator.parse(nullish, BigInt(123)).ok).toBe(false);
    });

    test('invalid nullish with date', () => {
      expect(schemaValidator.parse(nullish, new Date()).ok).toBe(false);
    });
  });

  describe('null', () => {
    test('valid null with null', () => {
      compare(schemaValidator.parse(null_, null), {
        ok: true,
        value: null
      });
    });

    test('invalid null with undefined', () => {
      expect(schemaValidator.parse(null_, undefined).ok).toBe(false);
    });

    test('invalid null with string', () => {
      expect(schemaValidator.parse(null_, 'hello').ok).toBe(false);
    });

    test('invalid null with number', () => {
      expect(schemaValidator.parse(null_, 123).ok).toBe(false);
    });

    test('invalid null with boolean', () => {
      expect(schemaValidator.parse(null_, true).ok).toBe(false);
    });

    test('invalid null with object', () => {
      expect(schemaValidator.parse(null_, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid null with array', () => {
      expect(schemaValidator.parse(null_, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid null with symbol', () => {
      expect(schemaValidator.parse(null_, Symbol('hello')).ok).toBe(false);
    });

    test('invalid null with bigint', () => {
      expect(schemaValidator.parse(null_, BigInt(123)).ok).toBe(false);
    });

    test('invalid null with date', () => {
      expect(schemaValidator.parse(null_, new Date()).ok).toBe(false);
    });
  });

  describe('void', () => {
    test('invalid void with null', () => {
      expect(schemaValidator.parse(void_, null).ok).toBe(false);
    });

    test('valid void with undefined', () => {
      compare(schemaValidator.parse(void_, undefined), {
        ok: true,
        value: undefined
      });
    });

    test('invalid void with string', () => {
      expect(schemaValidator.parse(void_, 'hello').ok).toBe(false);
    });

    test('invalid void with number', () => {
      expect(schemaValidator.parse(void_, 123).ok).toBe(false);
    });

    test('invalid void with boolean', () => {
      expect(schemaValidator.parse(void_, true).ok).toBe(false);
    });

    test('invalid void with object', () => {
      expect(schemaValidator.parse(void_, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid void with array', () => {
      expect(schemaValidator.parse(void_, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid void with symbol', () => {
      expect(schemaValidator.parse(void_, Symbol('hello')).ok).toBe(false);
    });

    test('invalid void with bigint', () => {
      expect(schemaValidator.parse(void_, BigInt(123)).ok).toBe(false);
    });

    test('invalid void with date', () => {
      expect(schemaValidator.parse(void_, new Date()).ok).toBe(false);
    });
  });

  describe('undefined', () => {
    test('invalid undefined with null', () => {
      expect(schemaValidator.parse(undefined_, null).ok).toBe(false);
    });

    test('valid undefined with undefined', () => {
      compare(schemaValidator.parse(undefined_, undefined), {
        ok: true,
        value: undefined
      });
    });

    test('invalid undefined with string', () => {
      expect(schemaValidator.parse(undefined_, 'hello').ok).toBe(false);
    });

    test('invalid undefined with number', () => {
      expect(schemaValidator.parse(undefined_, 123).ok).toBe(false);
    });

    test('invalid undefined with boolean', () => {
      expect(schemaValidator.parse(undefined_, true).ok).toBe(false);
    });

    test('invalid undefined with object', () => {
      expect(schemaValidator.parse(undefined_, { hello: 'world' }).ok).toBe(
        false
      );
    });

    test('invalid undefined with array', () => {
      expect(schemaValidator.parse(undefined_, ['hello', 'world']).ok).toBe(
        false
      );
    });

    test('invalid undefined with symbol', () => {
      expect(schemaValidator.parse(undefined_, Symbol('hello')).ok).toBe(false);
    });

    test('invalid undefined with bigint', () => {
      expect(schemaValidator.parse(undefined_, BigInt(123)).ok).toBe(false);
    });

    test('invalid undefined with date', () => {
      expect(schemaValidator.parse(undefined_, new Date()).ok).toBe(false);
    });
  });

  describe('any', () => {
    test('valid any with string', () => {
      compare(schemaValidator.parse(any, 'hello'), {
        ok: true,
        value: 'hello'
      });
    });

    test('valid any with number', () => {
      compare(schemaValidator.parse(any, 123), {
        ok: true,
        value: 123
      });
    });

    test('valid any with boolean', () => {
      compare(schemaValidator.parse(any, true), {
        ok: true,
        value: true
      });
    });

    test('valid any with object', () => {
      compare(schemaValidator.parse(any, { hello: 'world' }), {
        ok: true,
        value: { hello: 'world' }
      });
    });

    test('valid any with array', () => {
      compare(schemaValidator.parse(any, ['hello', 'world']), {
        ok: true,
        value: ['hello', 'world']
      });
    });

    test('valid any with null', () => {
      compare(schemaValidator.parse(any, null), {
        ok: true,
        value: null
      });
    });

    test('valid any with undefined', () => {
      compare(schemaValidator.parse(any, undefined), {
        ok: true,
        value: undefined
      });
    });

    test('valid any with symbol', () => {
      const symbolTest = Symbol('hello');
      compare(schemaValidator.parse(any, symbolTest), {
        ok: true,
        value: symbolTest
      });
    });

    test('valid any with bigint', () => {
      const parsed = schemaValidator.parse(any, BigInt(123));
      if (parsed.ok) {
        compare(parsed.value, BigInt(123));
      } else {
        expect.fail('Expected parsed.ok to be true');
      }
    });

    test('valid any with date', () => {
      const dateTest = new Date();
      compare(schemaValidator.parse(any, dateTest), {
        ok: true,
        value: dateTest
      });
    });
  });

  describe('unknown', () => {
    test('valid unknown with string', () => {
      compare(schemaValidator.parse(unknown, 'hello'), {
        ok: true,
        value: 'hello'
      });
    });

    test('valid unknown with number', () => {
      compare(schemaValidator.parse(unknown, 123), {
        ok: true,
        value: 123
      });
    });

    test('valid unknown with boolean', () => {
      compare(schemaValidator.parse(unknown, true), {
        ok: true,
        value: true
      });
    });

    test('valid unknown with object', () => {
      compare(schemaValidator.parse(unknown, { hello: 'world' }), {
        ok: true,
        value: { hello: 'world' }
      });
    });

    test('valid unknown with array', () => {
      compare(schemaValidator.parse(unknown, ['hello', 'world']), {
        ok: true,
        value: ['hello', 'world']
      });
    });

    test('valid unknown with null', () => {
      compare(schemaValidator.parse(unknown, null), {
        ok: true,
        value: null
      });
    });

    test('valid unknown with undefined', () => {
      compare(schemaValidator.parse(unknown, undefined), {
        ok: true,
        value: undefined
      });
    });

    test('valid unknown with symbol', () => {
      const symbolTest = Symbol('hello');
      compare(schemaValidator.parse(unknown, symbolTest), {
        ok: true,
        value: symbolTest
      });
    });

    test('valid unknown with bigint', () => {
      const parsed = schemaValidator.parse(unknown, BigInt(123));
      if (parsed.ok) {
        compare(parsed.value, BigInt(123));
      } else {
        expect.fail('Expected parsed.ok to be true');
      }
    });

    test('valid unknown with date', () => {
      const dateTest = new Date();
      compare(schemaValidator.parse(unknown, dateTest), {
        ok: true,
        value: dateTest
      });
    });
  });

  describe('never', () => {
    test('invalid never with string', () => {
      expect(schemaValidator.parse(never, 'hello').ok).toBe(false);
    });

    test('invalid never with number', () => {
      expect(schemaValidator.parse(never, 123).ok).toBe(false);
    });

    test('invalid never with boolean', () => {
      expect(schemaValidator.parse(never, true).ok).toBe(false);
    });

    test('invalid never with object', () => {
      expect(schemaValidator.parse(never, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid never with array', () => {
      expect(schemaValidator.parse(never, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid never with null', () => {
      expect(schemaValidator.parse(never, null).ok).toBe(false);
    });

    test('invalid never with undefined', () => {
      expect(schemaValidator.parse(never, undefined).ok).toBe(false);
    });

    test('invalid never with symbol', () => {
      expect(schemaValidator.parse(never, Symbol('hello')).ok).toBe(false);
    });

    test('invalid never with bigint', () => {
      expect(schemaValidator.parse(never, BigInt(123)).ok).toBe(false);
    });

    test('invalid never with date', () => {
      expect(schemaValidator.parse(never, new Date()).ok).toBe(false);
    });
  });

  describe('binary', () => {
    test('valid binary with string', () => {
      const base64String = Buffer.from('Hello World').toString('base64');
      compare(schemaValidator.parse(binary, base64String), {
        ok: true,
        value: new TextEncoder().encode('Hello World')
      });
    });

    test('invalid binary with number', () => {
      expect(schemaValidator.parse(binary, 123).ok).toBe(false);
    });

    test('invalid binary with boolean', () => {
      expect(schemaValidator.parse(binary, true).ok).toBe(false);
    });

    test('invalid binary with object', () => {
      expect(schemaValidator.parse(binary, { hello: 'world' }).ok).toBe(false);
    });

    test('invalid binary with array', () => {
      expect(schemaValidator.parse(binary, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid binary with null', () => {
      expect(schemaValidator.parse(binary, null).ok).toBe(false);
    });

    test('invalid binary with undefined', () => {
      expect(schemaValidator.parse(binary, undefined).ok).toBe(false);
    });

    test('invalid binary with symbol', () => {
      expect(schemaValidator.parse(binary, Symbol('hello')).ok).toBe(false);
    });

    test('invalid binary with bigint', () => {
      expect(schemaValidator.parse(binary, BigInt(123)).ok).toBe(false);
    });

    test('invalid binary with date', () => {
      expect(schemaValidator.parse(binary, new Date()).ok).toBe(false);
    });
  });

  describe('file', () => {
    test('valid file with string', async () => {
      const parsed = schemaValidator.parse(file, Buffer.from('Hello World'));
      expect(parsed.ok).toBe(true);
      if (parsed.ok) {
        expect(await parsed.value.text()).toBe('Hello World');
      }
    });

    test('invalid file with number', () => {
      expect(schemaValidator.parse(file, 123).ok).toBe(false);
    });

    test('invalid file with boolean', () => {
      expect(schemaValidator.parse(file, true).ok).toBe(false);
    });

    test('invalid file with object', () => {
      expect(
        schemaValidator.parse(file, {
          hello: 'world'
        }).ok
      ).toBe(false);
    });

    test('invalid file with array', () => {
      expect(schemaValidator.parse(file, ['hello', 'world']).ok).toBe(false);
    });

    test('invalid file with null', () => {
      expect(schemaValidator.parse(file, null).ok).toBe(false);
    });

    test('invalid file with undefined', () => {
      expect(schemaValidator.parse(file, undefined).ok).toBe(false);
    });

    test('invalid file with symbol', () => {
      expect(schemaValidator.parse(file, Symbol('hello')).ok).toBe(false);
    });

    test('invalid file with bigint', () => {
      expect(schemaValidator.parse(file, BigInt(123)).ok).toBe(false);
    });

    test('invalid file with date', () => {
      expect(schemaValidator.parse(file, new Date()).ok).toBe(false);
    });
  });

  describe('type', () => {
    test('valid type', () => {
      compare(schemaValidator.parse(type<string>(), 'hello'), {
        ok: true,
        value: 'hello'
      });
    });
  });
});

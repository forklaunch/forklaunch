import { logs } from '@opentelemetry/api-logs';
import type { Mock } from 'vitest';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { logger, meta } from '../src/http/telemetry/pinoLogger';

describe('PinoLogger Error Handling', () => {
  let testLogger: ReturnType<typeof logger>;
  let consoleSpy: Mock;
  let mockEmit: Mock;

  beforeEach(() => {
    mockEmit = vi.fn();
    vi.spyOn(logs, 'getLogger').mockReturnValue({
      enabled: () => true,
      emit: mockEmit
    });
    testLogger = logger('debug');
    consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('Circular Reference Handling', () => {
    it('should handle circular references without crashing', () => {
      const circularObj: Record<string, unknown> = { name: 'test' };
      circularObj.self = circularObj;

      expect(() => {
        testLogger.info('Testing circular reference', circularObj);
      }).not.toThrow();
    });

    it('should handle deeply nested circular references', () => {
      const obj1: Record<string, unknown> = { id: 1 };
      const obj2: Record<string, unknown> = { id: 2, parent: obj1 };
      obj1.child = obj2;

      expect(() => {
        testLogger.error('Deep circular reference', obj1, obj2);
      }).not.toThrow();
    });
  });

  describe('Non-serializable Objects', () => {
    it('should handle functions without crashing', () => {
      const testFunction = () => console.log('test');

      expect(() => {
        testLogger.warn('Function object', { fn: testFunction });
      }).not.toThrow();
    });

    it('should handle undefined and null values correctly', () => {
      expect(() => {
        testLogger.debug('Null and undefined', null, undefined, {
          key: undefined
        });
      }).not.toThrow();
    });

    it('should handle symbols without crashing', () => {
      const sym = Symbol('test');

      expect(() => {
        testLogger.trace('Symbol object', { symbol: sym });
      }).not.toThrow();
    });
  });

  describe('OpenTelemetry Error Handling', () => {
    it('should fallback to console logging when OpenTelemetry fails', async () => {
      const { logs } = await import('@opentelemetry/api-logs');
      const mockEmit = vi.fn().mockImplementation(() => {
        throw new Error('OpenTelemetry emission failed');
      });

      (logs.getLogger as ReturnType<typeof vi.fn>).mockReturnValue({
        emit: mockEmit
      });

      testLogger.error('Test message with OTel failure');

      expect(consoleSpy).toHaveBeenCalledWith(
        'Failed to emit OpenTelemetry log:',
        expect.any(Error)
      );
      expect(console.log).toHaveBeenCalled();
    });
  });

  describe('Complex Object Handling', () => {
    it('should handle mixed data types in a single log call', () => {
      const complexData = {
        string: 'test',
        number: 42,
        boolean: true,
        array: [1, 2, 3],
        nested: {
          deep: {
            value: 'nested'
          }
        }
      };

      expect(() => {
        testLogger.info(
          'Complex data',
          complexData,
          'additional string',
          123,
          true
        );
      }).not.toThrow();
    });

    it('should handle Error objects properly', () => {
      const error = new Error('Test error message');
      error.stack = 'Mock stack trace';

      expect(() => {
        testLogger.error('Error occurred', error);
      }).not.toThrow();
    });

    it('should handle Date objects', () => {
      const date = new Date();

      expect(() => {
        testLogger.info('Date object', { timestamp: date });
      }).not.toThrow();
    });
  });

  describe('LoggerMeta Handling', () => {
    it('should properly filter and include metadata', () => {
      const metadata = meta({ requestId: '123', userId: 'user456' });

      expect(() => {
        testLogger.info('Message with metadata', metadata, 'additional info');
      }).not.toThrow();
    });

    it('should handle multiple metadata objects', () => {
      const meta1 = meta({ requestId: '123' });
      const meta2 = meta({ userId: 'user456' });

      expect(() => {
        testLogger.debug('Multiple metadata', meta1, meta2, 'message');
      }).not.toThrow();
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty message', () => {
      expect(() => {
        testLogger.info('');
      }).not.toThrow();
    });

    it('should handle only metadata without message', () => {
      const metadata = meta({ key: 'value' });

      expect(() => {
        testLogger.warn(metadata);
      }).not.toThrow();
    });

    it('should handle very large objects', () => {
      const largeObject = {
        data: new Array(1000)
          .fill(0)
          .map((_, i) => ({ id: i, value: `item-${i}` }))
      };

      expect(() => {
        testLogger.debug('Large object', largeObject);
      }).not.toThrow();
    });

    it('should handle BigInt values', () => {
      const bigIntValue = BigInt(9007199254740991);

      expect(() => {
        testLogger.info('BigInt value', { value: bigIntValue });
      }).not.toThrow();
    });
  });

  describe('Child Logger Error Handling', () => {
    it('should create child loggers that inherit error handling', () => {
      const childLogger = testLogger.child({ service: 'test-service' });
      const circularObj: Record<string, unknown> = { name: 'child-test' };
      circularObj.self = circularObj;

      expect(() => {
        childLogger.error('Child logger with circular ref', circularObj);
      }).not.toThrow();
    });

    it('should handle nested child loggers', () => {
      const child1 = testLogger.child({ level1: 'test' });
      const child2 = child1.child({ level2: 'nested' });

      expect(() => {
        child2.info('Nested child logger', { complex: { data: 'test' } });
      }).not.toThrow();
    });
  });

  describe('Error Object Serialization', () => {
    it('should capture error message, name, and stack', () => {
      const error = new Error('Test error message');
      error.name = 'CustomError';

      testLogger.error('An error occurred', error);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.attributes.err).toBeDefined();
      expect(emitCall.attributes.err.message).toBe('Test error message');
      expect(emitCall.attributes.err.name).toBe('CustomError');
      expect(emitCall.attributes.err.stack).toBeDefined();
      expect(typeof emitCall.attributes.err.stack).toBe('string');
    });

    it('should capture custom properties on Error objects', () => {
      const error = new Error('Test error') as Error & {
        statusCode: number;
        requestId: string;
      };
      error.statusCode = 500;
      error.requestId = 'req-123';

      testLogger.error('Error with custom props', error);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.attributes.err.statusCode).toBe(500);
      expect(emitCall.attributes.err.requestId).toBe('req-123');
    });

    it('should handle Error as first argument', () => {
      const error = new Error('Primary error');

      testLogger.error(error);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.attributes.err).toBeDefined();
      expect(emitCall.attributes.err.message).toBe('Primary error');
    });

    it('should handle multiple Error objects', () => {
      const error1 = new Error('First error');
      const error2 = new Error('Second error');

      testLogger.error('Multiple errors', error1, error2);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      // The second error should overwrite the first due to Object.assign
      expect(emitCall.attributes.err.message).toBe('Second error');
    });

    it('should handle Error with other metadata', () => {
      const error = new Error('Test error');
      const metadata = meta({ requestId: 'req-456', userId: 'user-789' });

      testLogger.error('Error with metadata', error, metadata);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.attributes.err).toBeDefined();
      expect(emitCall.attributes.err.message).toBe('Test error');
      expect(emitCall.attributes.requestId).toBe('req-456');
      expect(emitCall.attributes.userId).toBe('user-789');
    });

    it('should handle Error with additional objects', () => {
      const error = new Error('Database error');
      const context = { table: 'users', operation: 'INSERT' };

      testLogger.error('Database operation failed', error, context);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.attributes.err.message).toBe('Database error');
      expect(emitCall.attributes.table).toBe('users');
      expect(emitCall.attributes.operation).toBe('INSERT');
    });

    it('should handle TypeError objects', () => {
      const error = new TypeError('Type error occurred');

      testLogger.error('Type error', error);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.attributes.err.message).toBe('Type error occurred');
      expect(emitCall.attributes.err.name).toBe('TypeError');
    });

    it('should handle RangeError objects', () => {
      const error = new RangeError('Range error occurred');

      testLogger.error('Range error', error);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.attributes.err.message).toBe('Range error occurred');
      expect(emitCall.attributes.err.name).toBe('RangeError');
    });

    it('should handle custom Error subclasses', () => {
      class CustomAppError extends Error {
        constructor(
          message: string,
          public code: string,
          public statusCode: number
        ) {
          super(message);
          this.name = 'CustomAppError';
        }
      }

      const error = new CustomAppError('Application error', 'APP_001', 400);

      testLogger.error('Custom error occurred', error);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.attributes.err.message).toBe('Application error');
      expect(emitCall.attributes.err.name).toBe('CustomAppError');
      expect(emitCall.attributes.err.code).toBe('APP_001');
      expect(emitCall.attributes.err.statusCode).toBe(400);
    });

    it('should handle Error in info level', () => {
      const error = new Error('Info level error');

      testLogger.info('Info with error', error);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.severityText).toBe('info');
      expect(emitCall.attributes.err.message).toBe('Info level error');
    });

    it('should handle Error in warn level', () => {
      const error = new Error('Warning error');

      testLogger.warn('Warning with error', error);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.severityText).toBe('warn');
      expect(emitCall.attributes.err.message).toBe('Warning error');
    });

    it('should handle Error in debug level', () => {
      const error = new Error('Debug error');

      testLogger.debug('Debug with error', error);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      expect(emitCall.severityText).toBe('debug');
      expect(emitCall.attributes.err.message).toBe('Debug error');
    });

    it('should include error in formatted body for pretty printing', () => {
      const error = new Error('Pretty print error');

      testLogger.error('Error message', error);

      expect(mockEmit).toHaveBeenCalled();
      const emitCall = mockEmit.mock.calls[0][0];

      // The body should contain formatted error information
      expect(emitCall.body).toContain('ERROR');
      expect(emitCall.body).toContain('Error message');
    });
  });
});

import { isNever } from '@forklaunch/common';
import { trace } from '@opentelemetry/api';
import { AnyValueMap, logs } from '@opentelemetry/api-logs';
import pino, { LevelWithSilent, LevelWithSilentOrString, Logger } from 'pino';
import { isLoggerMeta } from '../guards/isLoggerMeta';
import { LogFn, LoggerMeta } from '../types/openTelemetryCollector.types';

export type { LevelWithSilent, LevelWithSilentOrString, Logger };

export function meta(meta: Record<string, unknown>) {
  return meta as LoggerMeta;
}

function mapSeverity(level: LevelWithSilent) {
  switch (level) {
    case 'silent':
      return 0;
    case 'trace':
      return 1;
    case 'debug':
      return 5;
    case 'info':
      return 9;
    case 'warn':
      return 13;
    case 'error':
      return 17;
    case 'fatal':
      return 21;
    default:
      isNever(level);
      return 0;
  }
}

type LoggableArg = string | number | boolean | object | null | undefined;

/**
 * Normalize a list of arguments into a single metadata object and message string.
 * @param args - List of log arguments like console.log('msg', { a }, { b }, 123)
 * @returns [metadataObject, messageString]
 */
function normalizeLogArgs(
  args: LoggableArg[]
): [Record<string, unknown>, string] {
  let message = '';
  const metaObjects: Record<string, unknown>[] = [];

  for (const arg of args) {
    if (typeof arg === 'string' && message === '') {
      message = arg;
    } else if (arg instanceof Error) {
      metaObjects.push({
        err: {
          message: arg.message,
          name: arg.name,
          stack: arg.stack,
          ...(Object.keys(arg).length > 0 ? arg : {})
        }
      });
    } else if (arg && typeof arg === 'object' && !Array.isArray(arg)) {
      metaObjects.push(arg as Record<string, unknown>);
    } else {
      message += ` ${String(arg)}`;
    }
  }

  const metadata = Object.assign({}, ...metaObjects);
  return [metadata, message.trim()];
}

/**
 * Safely formats arguments for pretty printing with error handling
 * @param level - Log level
 * @param args - Arguments to format
 * @param timestamp - Optional timestamp
 * @returns Formatted string or fallback message
 */
function safePrettyFormat(
  level: LevelWithSilent,
  args: LoggableArg[],
  timestamp?: string
): string {
  try {
    const [metadata, message] = normalizeLogArgs(args);

    // Return formatted message with level and timestamp
    const formattedTimestamp = timestamp || new Date().toISOString();
    return `[${formattedTimestamp}] ${level.toUpperCase()}: ${message}${
      Object.keys(metadata).length > 0
        ? `\n${JSON.stringify(metadata, null, 2)}`
        : ''
    }`;
  } catch (error) {
    // Ultimate fallback for any serialization errors
    const fallbackMessage = args
      .map((arg) => {
        try {
          if (typeof arg === 'string') return arg;
          if (arg === null) return 'null';
          if (arg === undefined) return 'undefined';
          return JSON.stringify(arg);
        } catch {
          return '[Circular/Non-serializable Object]';
        }
      })
      .join(' ');

    return `[${new Date().toISOString()}] ${level.toUpperCase()}: ${fallbackMessage} [Pretty Print Error: ${error instanceof Error ? error.message : 'Unknown error'}]`;
  }
}

export class PinoLogger {
  _pinoLogger: Logger;
  _meta: AnyValueMap;

  constructor(level: LevelWithSilentOrString, meta: AnyValueMap = {}) {
    this._pinoLogger = pino({
      level: level || 'info',
      formatters: {
        level(label) {
          return { level: label };
        }
      },
      timestamp: pino.stdTimeFunctions.isoTime,
      transport: {
        target: 'pino-pretty',
        options: {
          colorize: true,
          errorLikeObjectKeys: ['err', 'error'],
          ignore: 'pid,hostname',
          translateTime: 'SYS:standard'
        }
      }
    });
    this._meta = meta;
  }

  log(level: LevelWithSilent, ...args: (string | unknown | LoggerMeta)[]) {
    let meta: AnyValueMap = {};

    const filteredArgs = args.filter((arg) => {
      if (isLoggerMeta(arg)) {
        Object.assign(meta, arg);
        return false;
      }
      return true;
    }) as LoggableArg[];

    const [errorMetadata] = normalizeLogArgs(filteredArgs);
    Object.assign(meta, errorMetadata);

    const activeSpan = trace.getActiveSpan();
    if (activeSpan) {
      const activeSpanContext = activeSpan.spanContext();
      meta.trace_id = activeSpanContext.traceId;
      meta.span_id = activeSpanContext.spanId;
      meta.trace_flags = activeSpanContext.traceFlags;
      meta = {
        // @ts-expect-error accessing private property
        ...activeSpan.attributes,
        ...meta
      };
    }

    meta = {
      'api.name': 'none',
      'correlation.id': 'none',
      ...meta
    };

    this._pinoLogger[level](...normalizeLogArgs(filteredArgs));

    const formattedBody = safePrettyFormat(level, filteredArgs);

    try {
      logs.getLogger(process.env.OTEL_SERVICE_NAME ?? 'unknown').emit({
        severityText: level,
        severityNumber: mapSeverity(level),
        body: formattedBody,
        attributes: { ...this._meta, ...meta }
      });
    } catch (error) {
      console.error('Failed to emit OpenTelemetry log:', error);
      console.log(
        `[${new Date().toISOString()}] ${level.toUpperCase()}:`,
        ...filteredArgs
      );
    }
  }

  error: LogFn = (
    msg: string | unknown | LoggerMeta,
    ...args: (string | unknown | LoggerMeta)[]
  ) => this.log('error', msg, ...args);

  info: LogFn = (
    msg: string | unknown | LoggerMeta,
    ...args: (string | unknown | LoggerMeta)[]
  ) => this.log('info', msg, ...args);

  debug: LogFn = (
    msg: string | unknown | LoggerMeta,
    ...args: (string | unknown | LoggerMeta)[]
  ) => this.log('debug', msg, ...args);
  warn: LogFn = (
    msg: string | unknown | LoggerMeta,
    ...args: (string | unknown | LoggerMeta)[]
  ) => this.log('warn', msg, ...args);

  trace: LogFn = (
    msg: string | unknown | LoggerMeta,
    ...args: (string | unknown | LoggerMeta)[]
  ) => this.log('trace', msg, ...args);

  child(meta: AnyValueMap = {}) {
    return new PinoLogger(this._pinoLogger.level, { ...this._meta, ...meta });
  }

  getBaseLogger() {
    return this._pinoLogger;
  }
}

export function logger(level: LevelWithSilentOrString, meta: AnyValueMap = {}) {
  return new PinoLogger(level, meta);
}

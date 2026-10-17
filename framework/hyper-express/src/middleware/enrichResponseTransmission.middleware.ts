import { toPlainString } from '@forklaunch/common';
import {
  enrichExpressLikeSend,
  ForklaunchNextFunction,
  ForklaunchSendableData,
  ParamsDictionary,
  VersionSchema
} from '@forklaunch/core/http';
import { AnySchemaValidator } from '@forklaunch/validator';
import { ParsedQs } from 'qs';
import { InternalRequest, InternalResponse } from '../types/hyperExpress.types';

/**
 * Middleware to enrich the response transmission by intercepting and parsing responses before they are sent.
 * This middleware enhances Hyper-Express's response handling by adding additional functionality to the send, json,
 * and setHeader methods. It also handles CORS and response status codes.
 *
 * @template SV - Schema validator type extending AnySchemaValidator
 * @param {InternalRequest} req - The Hyper-Express request object with additional type information
 * @param {InternalResponse} res - The Hyper-Express response object with additional type information
 * @param {ForklaunchNextFunction} next - The next middleware function in the chain
 *
 * @example
 * ```typescript
 * app.use(enrichResponseTransmission);
 *
 * app.get('/users', (req, res) => {
 *   // Response will be automatically enriched
 *   res.json({ users: [] });
 * });
 * ```
 */
export function enrichResponseTransmission<SV extends AnySchemaValidator>(
  req: InternalRequest<
    SV,
    ParamsDictionary,
    Record<string, unknown>,
    ParsedQs,
    Record<string, string>,
    Record<string, unknown>,
    string,
    Record<string, unknown>
  >,
  res: InternalResponse<
    Record<number, unknown>,
    Record<string, string>,
    Record<string, unknown>,
    string
  >,
  next: ForklaunchNextFunction
) {
  const originalSend = res.send;
  const originalJson = res.json;
  const originalSetHeader = res.setHeader;

  /**
   * Intercepts and enhances the JSON response method.
   * Adds additional processing and type validation before sending the response.
   * Handles CORS and response status codes.
   *
   * @template T - The type of the response data, must be a record with string keys
   * @param {T} data - The data to send in the response
   * @returns {T} - The processed response data
   *
   * @example
   * ```typescript
   * res.json({ id: 1, name: 'John' });
   * ```
   */
  res.json = function <T extends Record<string, unknown>>(data: T) {
    res.statusCode = Number(res._status_code);
    enrichExpressLikeSend<
      SV,
      ParamsDictionary,
      Record<number, unknown>,
      Record<string, unknown>,
      ParsedQs,
      Record<string, string>,
      Record<string, string>,
      Record<string, unknown>,
      VersionSchema<SV, 'middleware'>,
      Record<string, unknown>
    >(
      this,
      req,
      res,
      originalJson,
      originalSend,
      data,
      !res.cors && ((res._cork && !res._corked) || !res._cork)
    );
    return data;
  };

  /**
   * Intercepts and enhances the send response method.
   * Adds additional processing and error handling before sending the response.
   * If bodyData is not set, uses the provided data as the body.
   * Handles CORS and response status codes.
   *
   * @param {unknown} data - The data to send in the response
   * @returns {Response} - The Hyper-Express response object
   *
   * @example
   * ```typescript
   * res.send('Hello World');
   * ```
   */
  res.send = function (data: ForklaunchSendableData) {
    res.statusCode = Number(res._status_code);
    if (res.sent) {
      originalSend.call(this, data);
      return true;
    }
    enrichExpressLikeSend<
      SV,
      ParamsDictionary,
      Record<number, unknown>,
      Record<string, unknown>,
      ParsedQs,
      Record<string, string>,
      Record<string, string>,
      Record<string, unknown>,
      VersionSchema<SV, 'middleware'>,
      Record<string, unknown>
    >(
      this,
      req,
      res,
      originalSend,
      originalSend,
      data,
      !res.cors && ((res._cork && !res._corked) || !res._cork)
    );
    res.sent = true;
    return true;
  };

  /**
   * Intercepts and enhances the setHeader method.
   * Automatically stringifies non-string values before setting the header.
   *
   * @param {string} name - The name of the header
   * @param {unknown | unknown[]} value - The header value(s) to set
   * @returns {void}
   *
   * @example
   * ```typescript
   * res.setHeader('X-Custom-Header', { value: 123 }); // Will be stringified
   * res.setHeader('Accept', ['application/json', 'text/plain']);
   * ```
   */
  res.setHeader = function (name: string, value: unknown | unknown[]) {
    let stringifiedValue;
    if (Array.isArray(value)) {
      stringifiedValue = value.map((v) => toPlainString(v)).join('\n');
    } else {
      stringifiedValue = toPlainString(value);
    }
    return originalSetHeader.call(this, name, stringifiedValue);
  };

  res.sseEmitter = function (
    emitter: () => AsyncGenerator<Record<string, unknown>>
  ) {
    const generator = emitter();
    enrichExpressLikeSend<
      SV,
      ParamsDictionary,
      Record<number, unknown>,
      Record<string, unknown>,
      ParsedQs,
      Record<string, string>,
      Record<string, string>,
      Record<string, unknown>,
      VersionSchema<SV, 'middleware'>,
      Record<string, unknown>
    >(
      this,
      req,
      res,
      originalSend,
      originalSend,
      generator,
      !res.cors && ((res._cork && !res._corked) || !res._cork)
    );
  };

  next();
}

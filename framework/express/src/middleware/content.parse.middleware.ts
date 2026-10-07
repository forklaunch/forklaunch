/**
 * Middleware function to parse the request body based on the discriminated parserType.
 * Uses Express built-ins where possible and Busboy for multipart form data.
 *
 * Supported parser types:
 * - json: Parses body as JSON (uses express.json())
 * - urlEncoded: Parses body as URL-encoded form data (uses express.urlencoded())
 * - text: Parses body as plain text (uses raw parser with text encoding)
 * - file: Parses body as binary buffer (uses raw parser)
 * - multipart: Parses body as multipart form data using Busboy, handling both files and fields
 *
 * Every body-parser-backed parse also captures the exact request bytes on
 * `req._rawBody` (via body-parser's `verify` hook), so handlers can perform
 * HMAC/webhook signature verification over the payload as sent, while
 * `req.body` remains the parsed value.
 *
 * @param {object} options - Configuration options
 * @param {number} options.limit - Size limit for the request body (default: '10mb')
 * @returns {Function} Express middleware function
 */
import { isNever } from '@forklaunch/common';
import { discriminateBody, HttpContractDetails } from '@forklaunch/core/http';
import { AnySchemaValidator } from '@forklaunch/validator';
import {
  Options,
  OptionsJson,
  OptionsText,
  OptionsUrlencoded
} from 'body-parser';
import Busboy, { BusboyConfig } from 'busboy';
import express, {
  NextFunction,
  Request,
  RequestHandler,
  Response
} from 'express';
import expressStatic from 'express-serve-static-core';
import { ParsedQs } from 'qs';
import { Range } from 'range-parser';
import { SetExportTypes } from '../types/export.types';

type ContentParseOptions = {
  busboy?: BusboyConfig;
  text?: OptionsText;
  json?: OptionsJson;
  urlencoded?: OptionsUrlencoded;
  raw?: Options;
};

type BodyParserVerify = (
  req: Request,
  res: Response,
  buf: Buffer,
  encoding: string
) => void;

/**
 * The content types body-parser matches by default for each parser type
 * (mirrors the defaults `discriminateBody` reports). A route-declared
 * `contentType` only overrides the parser's `type` matcher when it differs
 * from these, preserving body-parser's default matching semantics otherwise.
 */
const DEFAULT_CONTENT_TYPES = {
  json: 'application/json',
  text: 'text/plain',
  urlEncoded: 'application/x-www-form-urlencoded',
  file: 'application/octet-stream',
  multipart: 'multipart/form-data'
} as const;

function contentParse<SV extends AnySchemaValidator>(
  options?: ContentParseOptions
) {
  // Options are resolved lazily on first request: at construction time the
  // application's options have not yet been merged into the owning router's
  // options (addRouterOptions runs at mount time), but req._globalOptions()
  // exposes the merged result at request time.
  let resolvedOptions: ContentParseOptions | undefined;
  const parserCache = new Map<string, RequestHandler>();

  const captureRawBody = (base?: { verify?: unknown }): BodyParserVerify => {
    const userVerify = base?.verify as BodyParserVerify | undefined;
    return (req, res, buf, encoding) => {
      userVerify?.(req, res, buf, encoding);
      (req as { _rawBody?: Buffer })._rawBody = buf;
    };
  };

  const buildParser = (
    parserType: 'json' | 'urlEncoded' | 'text' | 'file',
    contentTypeOverride: string | undefined
  ): RequestHandler => {
    const typeOverride = contentTypeOverride
      ? { type: contentTypeOverride }
      : {};
    switch (parserType) {
      case 'json':
        return express.json({
          ...resolvedOptions?.json,
          verify: captureRawBody(resolvedOptions?.json),
          ...typeOverride
        });
      case 'urlEncoded':
        return express.urlencoded({
          extended: true,
          ...resolvedOptions?.urlencoded,
          verify: captureRawBody(resolvedOptions?.urlencoded),
          ...typeOverride
        });
      case 'text':
        return express.text({
          ...resolvedOptions?.text,
          verify: captureRawBody(resolvedOptions?.text),
          ...typeOverride
        });
      case 'file':
        return express.raw({
          ...resolvedOptions?.raw,
          verify: captureRawBody(resolvedOptions?.raw),
          ...typeOverride
        });
      default:
        isNever(parserType);
        throw new Error('Unsupported parser type: ' + parserType);
    }
  };

  const parserFor = (
    parserType: 'json' | 'urlEncoded' | 'text' | 'file',
    contentType: string
  ): RequestHandler => {
    const contentTypeOverride =
      contentType !== DEFAULT_CONTENT_TYPES[parserType]
        ? contentType
        : undefined;
    const cacheKey = `${parserType}:${contentTypeOverride ?? ''}`;
    let parser = parserCache.get(cacheKey);
    if (!parser) {
      parser = buildParser(parserType, contentTypeOverride);
      parserCache.set(cacheKey, parser);
    }
    return parser;
  };

  return async (req: Request, res: Response, next: NextFunction) => {
    try {
      // `_rawBody` and the other fields read below are attached by earlier
      // middleware, not declared on Express' `Request`, so the augmented shape
      // shares no declared member with it.
      const coercedRequest = req as unknown as {
        schemaValidator: SV;
        contractDetails: HttpContractDetails<SV>;
        _globalOptions?: () => ContentParseOptions | undefined;
      };

      let contractBody;
      if (coercedRequest.contractDetails.versions) {
        contractBody = Object.values(coercedRequest.contractDetails.versions)[0]
          ?.body;
      } else {
        contractBody = coercedRequest.contractDetails.body;
      }

      const discriminatedBody = discriminateBody(
        coercedRequest.schemaValidator,
        contractBody
      );

      if (!discriminatedBody) {
        return next();
      }

      if (!resolvedOptions) {
        const globalOptions = coercedRequest._globalOptions?.();
        resolvedOptions = {
          busboy: globalOptions?.busboy ?? options?.busboy,
          text: globalOptions?.text ?? options?.text,
          json: globalOptions?.json ?? options?.json,
          urlencoded: globalOptions?.urlencoded ?? options?.urlencoded,
          raw: globalOptions?.raw ?? options?.raw
        };
      }

      switch (discriminatedBody.parserType) {
        case 'json':
        case 'urlEncoded':
        case 'text':
          return parserFor(
            discriminatedBody.parserType,
            discriminatedBody.contentType
          )(req, res, next);
        case 'file':
          return parserFor('file', discriminatedBody.contentType)(
            req,
            res,
            async (err) => {
              if (err) {
                return next(err);
              }
              next();
            }
          );
        case 'multipart': {
          const bb = Busboy({
            headers: req.headers,
            ...resolvedOptions?.busboy
          });
          const body: Record<string, unknown> = {};

          bb.on('file', (fieldname: string, file: NodeJS.ReadableStream) => {
            const chunks: Buffer[] = [];

            file.on('data', (chunk: Buffer) => {
              chunks.push(chunk);
            });

            file.on('end', () => {
              const fileBuffer = Buffer.concat(chunks);
              body[fieldname] = fileBuffer;
            });
          });

          bb.on('field', (fieldname: string, value: string) => {
            body[fieldname] = value;
          });

          bb.on('finish', () => {
            req.body = body;
            next();
          });

          bb.on('error', (err: Error) => {
            next(err);
          });

          req.pipe(bb);
          break;
        }
        default:
          isNever(discriminatedBody.parserType);
          throw new Error(
            'Unsupported parser type for body: ' + discriminatedBody.parserType
          );
      }
    } catch (error) {
      next(error);
    }
  };
}

export { contentParse };

// eslint-disable-next-line @typescript-eslint/no-unused-vars
type Dummy = SetExportTypes<ParsedQs, expressStatic.Express, Range>;

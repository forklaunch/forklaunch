import { isNever } from '@forklaunch/common';
import { discriminateBody, HttpContractDetails } from '@forklaunch/core/http';
import { MiddlewareHandler, Request } from '@forklaunch/hyper-express-fork';
import { AnySchemaValidator } from '@forklaunch/validator';
import { BusboyConfig } from 'busboy';

/**
 * Middleware function to parse the request body based on the content type.
 * Supports multiple content types including JSON, form data, text, binary, and multipart.
 *
 * Supported content types:
 * - application/json: Parses body as JSON
 * - application/x-www-form-urlencoded: Parses body as URL-encoded form data
 * - text/plain: Parses body as plain text
 * - application/octet-stream: Parses body as binary buffer
 * - multipart/form-data: Parses body as multipart form data, handling both files and fields
 *
 * @param {Request} req - The Hyper-Express request object
 * @returns {Promise<void>} - Promise that resolves when parsing is complete
 *
 * @example
 * ```typescript
 * // Use as middleware
 * app.use(contentParse);
 *
 * // Access parsed body in route handler
 * app.post('/upload', async (req, res) => {
 *   if (req.headers['content-type'] === 'multipart/form-data') {
 *     // Access uploaded file
 *     const file = req.body.fileField;
 *     console.log(file.name, file.type, file.buffer);
 *   }
 * });
 * ```
 */
export function contentParse<SV extends AnySchemaValidator>(options?: {
  busboy?: BusboyConfig;
}): MiddlewareHandler {
  return async (req: Request) => {
    const coercedRequest = req as unknown as {
      schemaValidator: SV;
      contractDetails: HttpContractDetails<SV>;
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

    if (discriminatedBody != null) {
      switch (discriminatedBody.parserType) {
        case 'file':
          req.body = await req.buffer();
          break;
        case 'json':
          req.body = await req.json();
          break;
        case 'multipart': {
          const body: Record<string, unknown> = {};
          await req.multipart(options?.busboy ?? {}, async (field) => {
            if (field.file) {
              const chunks: Uint8Array[] = [];
              for await (const chunk of field.file.stream) {
                const buf = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
                chunks.push(
                  new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength)
                );
              }
              const fileBuffer = Buffer.concat(chunks);
              body[field.name] = fileBuffer;
            } else {
              body[field.name] = field.value;
            }
          });
          req.body = body;
          break;
        }
        case 'text':
          req.body = await req.text();
          break;
        case 'urlEncoded':
          req.body = await req.urlencoded();
          break;
        default:
          isNever(discriminatedBody.parserType);
          throw new Error(
            'Unsupported parser type for body: ' + discriminatedBody.parserType
          );
      }
    }
  };
}

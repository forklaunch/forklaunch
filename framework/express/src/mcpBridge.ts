import { Request, Response } from 'express';
import { Readable } from 'stream';
import { ReadableStream as NodeReadableStream } from 'stream/web';

/**
 * Builds a fetch `Request` from an Express request so the shared MCP handler
 * can serve it. Uses the bytes captured by the content parser when a body was
 * already consumed, and reads the stream otherwise.
 */
export async function toFetchRequest(
  req: Request,
  protocol: 'http' | 'https'
): Promise<globalThis.Request> {
  const headers = new Headers();
  for (const [key, value] of Object.entries(req.headers)) {
    if (Array.isArray(value)) {
      value.forEach((v) => headers.append(key, v));
    } else if (value != null) {
      headers.set(key, value);
    }
  }

  let body: Buffer | undefined;
  if (req.method !== 'GET' && req.method !== 'HEAD') {
    const rawBody = (req as Request & { _rawBody?: unknown })._rawBody;
    if (Buffer.isBuffer(rawBody)) {
      body = rawBody;
    } else if (!req.readableEnded) {
      const chunks: Buffer[] = [];
      for await (const chunk of req) {
        chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
      }
      body = Buffer.concat(chunks);
    } else if (req.body !== undefined) {
      body = Buffer.from(JSON.stringify(req.body));
    }
  }

  return new globalThis.Request(
    `${protocol}://${req.headers.host ?? 'localhost'}${req.originalUrl}`,
    {
      method: req.method,
      headers,
      body: body ? new Uint8Array(body) : undefined
    }
  );
}

/**
 * Writes a fetch `Response` to an Express response, streaming the body so
 * Server-Sent Events reach the client as they are produced.
 */
export async function writeFetchResponse(
  response: globalThis.Response,
  res: Response
): Promise<void> {
  const headers: Record<string, string> = {};
  response.headers.forEach((value, key) => {
    headers[key] = value;
  });
  res.writeHead(response.status, headers);

  if (!response.body) {
    res.end();
    return;
  }

  res.flushHeaders();
  const body = Readable.fromWeb(
    response.body as unknown as NodeReadableStream<Uint8Array>
  );
  res.on('close', () => body.destroy());
  await new Promise<void>((resolve) => {
    body.on('end', resolve);
    body.on('error', () => {
      res.destroy();
      resolve();
    });
    body.on('close', resolve);
    body.pipe(res);
  });
}

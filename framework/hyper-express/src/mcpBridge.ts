import { Request, Response } from '@forklaunch/hyper-express-fork';
import http from 'http';
import { Readable } from 'stream';
import { ReadableStream as NodeReadableStream } from 'stream/web';

/**
 * Builds a fetch `Request` from a hyper-express request so the shared MCP
 * handler can serve it.
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
      headers.set(key, String(value));
    }
  }

  const body =
    req.method !== 'GET' && req.method !== 'HEAD'
      ? await req.buffer()
      : undefined;

  return new globalThis.Request(
    `${protocol}://${req.headers.host ?? 'localhost'}${req.url}`,
    {
      method: req.method,
      headers,
      body: body && body.length > 0 ? new Uint8Array(body) : undefined
    }
  );
}

/**
 * hyper-express requests are not Node `IncomingMessage`s. The MCP
 * `authenticate` hook only needs the method, URL and headers, so it gets a
 * stand-in carrying those.
 */
export function toAuthRequest(req: Request): http.IncomingMessage {
  return {
    method: req.method,
    url: req.url,
    headers: req.headers
  } as unknown as http.IncomingMessage;
}

/**
 * Writes a fetch `Response` to a hyper-express response, streaming the body so
 * Server-Sent Events reach the client as they are produced.
 */
export async function writeFetchResponse(
  response: globalThis.Response,
  res: Response
): Promise<void> {
  res.status(response.status);
  response.headers.forEach((value, key) => {
    res.header(key, value);
  });

  if (!response.body) {
    res.send();
    return;
  }

  await res.stream(
    Readable.fromWeb(response.body as unknown as NodeReadableStream<Uint8Array>)
  );
}

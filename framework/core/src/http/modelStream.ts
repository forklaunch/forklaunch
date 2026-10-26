import {
  ModelGatewayRequestError,
  type GatewayChatChunk
} from './modelGatewayClient';
/** Read bounded model events; partial/disconnected answers are not success. */
export async function* readModelStream(
  response: Response
): AsyncGenerator<GatewayChatChunk> {
  const reader = response.body?.getReader();
  if (!reader)
    throw new ModelGatewayRequestError(502, 'The model stream is empty.');
  const decoder = new TextDecoder();
  let buffer = '',
    bytes = 0;
  try {
    for (;;) {
      const part = await reader.read();
      if (part.done) break;
      bytes += part.value.byteLength;
      if (bytes > 512000) throw new Error('oversized');
      buffer += decoder.decode(part.value, { stream: true });
      let match: RegExpMatchArray | null;
      while ((match = buffer.match(/\r?\n\r?\n/))) {
        const event = buffer.slice(0, match.index);
        buffer = buffer.slice(match.index! + match[0].length);
        const data = event
          .split(/\r?\n/)
          .filter((line) => line.startsWith('data:'))
          .map((line) => line.slice(5).trimStart())
          .join('\n');
        if (!data) continue;
        if (data === '[DONE]') return;
        const value = JSON.parse(data);
        if (value.error || !Array.isArray(value.choices))
          throw new Error('invalid');
        yield value as GatewayChatChunk;
      }
    }
    throw new Error('interrupted');
  } catch {
    throw new ModelGatewayRequestError(
      502,
      'The model stream was interrupted. No automatic retry was sent.'
    );
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

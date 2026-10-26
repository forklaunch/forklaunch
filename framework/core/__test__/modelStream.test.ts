import { expect, it } from 'vitest';
import { readModelStream } from '../src/http/modelStream';
it('yields split chunks before completion and cancels early readers', async () => {
  let control: ReadableStreamDefaultController<Uint8Array>;
  let cancelled = false;
  const input = new ReadableStream<Uint8Array>({
    start(c) {
      control = c;
    },
    cancel() {
      cancelled = true;
    }
  });
  const stream = readModelStream(new Response(input));
  const first = stream.next();
  control!.enqueue(
    new TextEncoder().encode('data: {"choices":[{"delta":{"content":"hel')
  );
  control!.enqueue(new TextEncoder().encode('lo"}}]}\n\n'));
  expect((await first).value.choices[0].delta.content).toBe('hello');
  await stream.return(undefined);
  expect(cancelled).toBe(true);
});
it('refuses interrupted or failed streams instead of accepting partial output', async () => {
  for (const data of [
    'data: {"choices":[]}\n\n',
    'data: {"error":{"message":"secret"}}\n\n'
  ]) {
    await expect(
      (async () => {
        for await (const _ of readModelStream(new Response(data))) {
          /* consume */
        }
      })()
    ).rejects.toThrow('interrupted');
  }
});

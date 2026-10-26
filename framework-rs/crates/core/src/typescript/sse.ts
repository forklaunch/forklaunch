interface ForkLaunchResponseReader {
    read(): Promise<{ done: boolean; value?: Uint8Array }>;
    cancel(): Promise<void>;
    releaseLock(): void;
}

/** Web fetch uses ReadableStream; Node fetch adapters expose async iterable bytes. */
function responseReader(response: Response): ForkLaunchResponseReader {
    const body = response.body as unknown as {
        getReader?: () => ReadableStreamDefaultReader<Uint8Array>;
        [Symbol.asyncIterator]?: () => AsyncIterator<Uint8Array, unknown>;
        destroy?: () => void;
    } | null;
    if (!body) throw new TypeError('No response body for event stream');
    if (typeof body.getReader === 'function') return body.getReader();
    const createIterator = body[Symbol.asyncIterator];
    if (typeof createIterator !== 'function')
        throw new TypeError('Response body must expose a readable byte stream');
    const iterator = createIterator.call(body);
    return {
        async read() {
            const result = await iterator.next();
            if (result.done) return { done: true };
            if (!(result.value instanceof Uint8Array))
                throw new TypeError('Response stream yielded non-byte data');
            return { done: false, value: result.value };
        },
        async cancel() {
            // Destroy first to interrupt a pending read, including unstarted iterators.
            body.destroy?.();
            await iterator.return?.();
        },
        releaseLock() {}
    };
}

async function cancelResponseBody(response: Response): Promise<void> {
    if (!response.body) return;
    const reader = responseReader(response);
    try { await reader.cancel(); }
    finally { reader.releaseLock(); }
}

/** Pull-driven SSE decoding; iterator completion or cancellation releases the reader. */
function sseEvents(response: Response, maxEventBytes = 1048576, onClose: () => void = () => {}, validate?: (event: unknown) => unknown): AsyncGenerator<{
    data: unknown;
    id?: string;
}> {
    if (!response.body)
        throw new TypeError('No response body for event stream');
    const reader = responseReader(response);
    const decoder = new TextDecoder('utf-8', { ignoreBOM: true });
    let line: number[] = [];
    let data = '';
    let id: string | undefined;
    let skipLf = false;
    let firstLine = true;
    let frameBytes = 0;
    let released = false;
    async function close() {
        if (released)
            return;
        released = true;
        try {
            onClose();
            await reader.cancel();
        }
        catch { /* Preserve the original read/parse error. */ }
        finally {
            reader.releaseLock();
        }
    }
    async function* decode(): AsyncGenerator<{
        data: unknown;
        id?: string;
    }> {
        try {
            if (!Number.isSafeInteger(maxEventBytes) || maxEventBytes < 1)
                throw new RangeError('SSE event limit must be positive');
            while (true) {
                const { done, value } = await reader.read();
                if (done)
                    break;
                if (!value) throw new TypeError('Response stream yielded no byte data');
                for (const byte of value) {
                    if (skipLf) {
                        skipLf = false;
                        if (byte === 10)
                            continue;
                    }
                    if (++frameBytes > maxEventBytes)
                        throw new RangeError('SSE event exceeds configured limit');
                    if (byte !== 10 && byte !== 13) {
                        line.push(byte);
                        continue;
                    }
                    skipLf = byte === 13;
                    let text = decoder.decode(Uint8Array.from(line));
                    line = [];
                    if (firstLine) {
                        firstLine = false;
                        if (text.startsWith('\uFEFF'))
                            text = text.slice(1);
                    }
                    if (text === '') {
                        frameBytes = 0;
                        if (data !== '') {
                            const raw = data.slice(0, -1);
                            data = '';
                            // Non-JSON data stays text, as the universal SDK and
                            // the native codec deliver it.
                            let parsed: unknown;
                            try {
                                parsed = JSON.parse(raw);
                            }
                            catch {
                                parsed = raw;
                            }
                            const event = id === undefined ? { data: parsed } : { data: parsed, id };
                            const decoded = validate ? validate(event) : event;
                            yield decoded as { data: unknown; id?: string };
                        }
                        continue;
                    }
                    if (text.startsWith(':'))
                        continue;
                    const colon = text.indexOf(':');
                    const field = colon < 0 ? text : text.slice(0, colon);
                    let fieldValue = colon < 0 ? '' : text.slice(colon + 1);
                    if (fieldValue.startsWith(' '))
                        fieldValue = fieldValue.slice(1);
                    if (field === 'data')
                        data += `${fieldValue}\n`;
                    else if (field === 'id' && !fieldValue.includes('\0'))
                        id = fieldValue;
                }
            }
        }
        finally {
            await close();
        }
    }
    const iterator = decode();
    const finish = iterator.return.bind(iterator);
    const fail = iterator.throw.bind(iterator);
    iterator.return = async (value) => { await close(); return finish(value); };
    iterator.throw = async (error) => { await close(); return fail(error); };
    return iterator;
}

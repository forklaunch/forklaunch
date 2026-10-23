/** Credentials require TLS outside loopback unless a local development exception is explicit. */
function requireSecureHttpTransport(value: string, allowPlaintextDevelopment: boolean = false): void {
    const endpoint = new URL(value, typeof globalThis.location === 'undefined' ? undefined : globalThis.location.href);
    const loopback = endpoint.hostname === 'localhost' || endpoint.hostname === '[::1]'
        || /^127(?:\.\d{1,3}){3}$/.test(endpoint.hostname);
    if (endpoint.username || endpoint.password || endpoint.hash
        || !(endpoint.protocol === 'https:' || (endpoint.protocol === 'http:' && (loopback || allowPlaintextDevelopment === true)))) {
        throw new TypeError('HTTP TLS is required outside loopback development');
    }
}
// End HTTP transport admission.

/** Serialize Web upload bodies for adapters that do not consume standard Blob/FormData. */
async function compatibleFetchBody(body: BodyInit | undefined, headers: Headers, fetcher: typeof fetch): Promise<BodyInit | undefined> {
    if (fetcher === globalThis.fetch) return body;
    if (body instanceof Blob) return body.arrayBuffer();
    if (body instanceof FormData) {
        // The standard encoder owns MIME boundaries, names, files and CRLF framing.
        const encoded = new Response(body);
        const contentType = encoded.headers.get('content-type');
        if (!contentType) throw new TypeError('Multipart encoder did not select a content type');
        headers.set('content-type', contentType);
        return encoded.arrayBuffer();
    }
    return body;
}

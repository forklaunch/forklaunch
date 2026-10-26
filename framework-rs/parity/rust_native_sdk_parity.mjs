import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { appendFileSync, mkdirSync, mkdtempSync, symlinkSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const temporary = mkdtempSync(join(tmpdir(), 'forklaunch-native-sdk-parity-'));
const require = createRequire(import.meta.url);
function run(command, args, env = process.env) {
  const result = spawnSync(command, args, { cwd: temporary, env, encoding: 'utf8', timeout: 300000, maxBuffer: 16 * 1024 * 1024 });
  assert.equal(result.status, 0, `${result.error || ''}\n${result.stdout}\n${result.stderr}`);
}
let server;
try {
  run('cargo', ['test', '--quiet', '--manifest-path', join(root, 'framework-rs/Cargo.toml'), '-p', 'forklaunch-core','--features','build,asyncapi-import', '-p', 'forklaunch-core','--features','build,asyncapi-import', 'native_sdk_preserves'], { ...process.env, FORKLAUNCH_TEST_NATIVE_SDK_OUTPUT: temporary });
  writeFileSync(join(temporary, 'typed-usage.ts'), `
import { createForkLaunchSdk } from './typed-native-sdk';
export async function verifyTypes(sdk = createForkLaunchSdk()) {
  await sdk.media.file.postFile({ body: new Blob() });
  await sdk.media.binary.postBinary({ body: new ArrayBuffer(0) });
  await sdk.media.urlencoded.postUrlencoded({ body: { key: ['a', 'b'] } });
  await sdk.media.form.postForm({ body: { label: 'text' } });
  // @ts-expect-error Binary requests reject JSON objects.
  await sdk.media.binary.postBinary({ body: { wrong: true } });
  // @ts-expect-error Text requests reject byte arrays.
  await sdk.media.text.postText({ body: new Uint8Array() });
  const several = await sdk.media.responses.readSeveral({ params: { mode: 'created' } });
  if (several.code === 201) {
    const value: string = several.response.createdAt;
    // @ts-expect-error Status 201 does not contain the queued response.
    const wrong: number = several.response.jobId;
    void [value, wrong];
  }
  if (several.code === 202) { const value: number = several.response.jobId; void value; }
  if (several.code === 204) { const value: undefined = several.response; void value; }
  const error = await sdk.media.responses.readError();
  if (error.code === 409) {
    const code: string = error.response.errorCode;
    const mime: 'application/problem+json' = error.contentType;
    void [code, mime];
  }
  // @ts-expect-error Error-only contracts do not invent success statuses.
  if (error.code === 200) throw Error('invented success');
}
`);
  const tsc = process.env.TSC_BIN || join(root, 'framework/node_modules/.bin/tsc');
  const crossFetchPath = process.env.CROSS_FETCH_PATH || join(dirname(tsc), '../cross-fetch');
  const crossFetch = require(crossFetchPath);
  mkdirSync(join(temporary, 'node_modules'));
  symlinkSync(crossFetchPath, join(temporary, 'node_modules/cross-fetch'));
  // Generated SDKs import their schema runtime (Ajv); link the pinned packages.
  for (const name of ['ajv', 'ajv-formats', 'ajv-draft-04', 'qs', '@types']) symlinkSync(join(dirname(tsc), '..', name), join(temporary, 'node_modules', name));
  writeFileSync(join(temporary, 'tsconfig.json'), JSON.stringify({ compilerOptions: {
    strict: true, target: 'ES2022', module: 'NodeNext', moduleResolution: 'NodeNext',
    lib: ['ES2022', 'DOM', 'DOM.Iterable'], types: [], outDir: 'compiled'
  }, include: ['*.ts'] }));
  run(process.env.TSC_BIN || join(root, 'framework/node_modules/.bin/tsc'), ['--project', join(temporary, 'tsconfig.json')]);
  appendFileSync(join(temporary, 'native-sdk.rs'), String.raw`
fn main() {
    let client = ForkLaunchClient::new("http://localhost");
    let confidential=ForkLaunchClient::new("https://url-user:url-secret@example.test/path?token=query-secret");
    let mut diagnostic=confidential.post_text_request(PostTextRequest { body:Some("body-secret".into()), ..Default::default() });
    diagnostic.headers.insert("authorization".into(),"Bearer header-secret".into());
    diagnostic.query.insert("token".into(),"query-secret".into());
    let mut input=PostTextRequest { body:Some("body-secret".into()), ..Default::default() };
    input.params.insert("id".into(),"param-secret".into());
    input.query=diagnostic.query.clone();
    input.headers=diagnostic.headers.clone();
    let rendered=format!("{confidential:?} {diagnostic:?} {input:?}");
    for secret in ["url-user","url-secret","query-secret","body-secret","header-secret","param-secret"] { assert!(!rendered.contains(secret),"diagnostic leaked {secret}"); }
    assert!(rendered.contains("authorization"));
    let request = client.post_binary_request(PostBinaryRequest { body: Some(vec![0, 255, 1]), ..Default::default() });
    assert_eq!(request.body_bytes, Some(vec![0, 255, 1]));
    assert!(request.body.is_none());
    assert_eq!(request.body_content_type.as_deref(), Some("application/octet-stream"));
    let file = client.post_file_request(PostFileRequest { body: Some(vec![137, 80, 78, 71]), ..Default::default() });
    assert_eq!(file.body_content_type.as_deref(), Some("image/png"));
    assert_eq!(file.body_bytes, Some(vec![137, 80, 78, 71]));
    let text = client.post_text_request(PostTextRequest { body: Some("hello 水".into()), ..Default::default() });
    assert_eq!(text.body.as_deref(), Some("hello 水"));
    assert_eq!(text.body_content_type.as_deref(), Some("text/plain; charset=utf-8"));
    let form = MultipartForm::new().text("label", "hello 水").text("newlines", "a\nb\rc\r\nd")
        .part("file", MultipartPart::bytes(vec![0,255,1]).file_name("data.bin").mime_str("application/octet-stream").unwrap())
        .part("file", MultipartPart::bytes(Vec::new()).file_name("empty.bin"));
    let upload = client.post_form_request(PostFormRequest { body: Some(form), headers: [("Content-Type".into(), "incorrect".into())].into(), ..Default::default() });
    assert!(upload.headers["content-type"].starts_with("multipart/form-data; boundary="));
    assert!(!upload.headers.contains_key("Content-Type"));
    assert_eq!(upload.body_content_type.as_deref(), Some(upload.headers["content-type"].as_str()));
    std::fs::write(std::env::args().nth(2).unwrap(), &upload.headers["content-type"]).unwrap();
    let bytes = upload.body_bytes.unwrap();
    std::fs::write(std::env::args().nth(1).unwrap(), &bytes).unwrap();
    assert!(bytes.windows(3).any(|b| b == [0,255,1]));
    assert!(bytes.windows(10).any(|b| b == b"a\r\nb\r\nc\r\nd"));
    assert!(MultipartPart::bytes(vec![]).mime_str("text/plain\r\nInjected: true").is_err());
    let empty = client.post_form_request(PostFormRequest { body: Some(MultipartForm::new()), ..Default::default() });
    let boundary = empty.body_content_type.unwrap().split("boundary=").nth(1).unwrap().to_owned();
    assert_eq!(empty.body_bytes.unwrap(), format!("--{boundary}--\r\n").as_bytes());

}
`);
  run('rustc', ['--edition=2024', '-D', 'warnings', join(temporary, 'native-sdk.rs'), '-o', join(temporary, 'native-sdk')]);
  run(join(temporary, 'native-sdk'), [join(temporary, 'multipart-bytes'), join(temporary, 'multipart-content-type')]);
  const rustForm = await new Response(readFileSync(join(temporary, 'multipart-bytes')), { headers: { 'content-type': readFileSync(join(temporary, 'multipart-content-type'), 'utf8') } }).formData();
  assert.equal(rustForm.get('label'), 'hello 水');
  assert.equal(rustForm.get('newlines'), 'a\r\nb\r\nc\r\nd');
  assert.equal(rustForm.getAll('file').length, 2);
  assert.equal(rustForm.getAll('file')[0].name, 'data.bin');
  assert.deepEqual([...new Uint8Array(await rustForm.getAll('file')[0].arrayBuffer())], [0,255,1]);
  assert.equal(rustForm.getAll('file')[1].size, 0);
  server = createServer(async (request, response) => {
    if (request.url.startsWith('/responses/')) {
      const mode = request.url.slice('/responses/'.length);
      const send = (code, mime, value) => { response.writeHead(code, mime === undefined ? {} : { 'content-type': mime }); response.end(value); };
      if (mode === 'text') return send(200, 'text/plain; charset=utf-8', 'hello 水\n');
      if (mode === 'binary') return send(200, 'application/octet-stream', Buffer.from([0, 255, 13, 10]));
      if (mode === 'null') return send(200, 'application/json', 'null');
      if (mode === 'invalid') return send(200, 'application/json', '{broken');
      if (mode === 'empty') return send(204, 'application/json');
      if (mode === 'head') return send(200, 'application/json');
      if (mode === 'error') return send(409, 'application/problem+json', JSON.stringify({ errorCode: 'CONFLICT' }));
      if (mode === 'several/created') return send(201, 'application/json', JSON.stringify({ createdAt: 'today' }));
      if (mode === 'several/queued') return send(202, 'application/json', JSON.stringify({ jobId: 42 }));
      if (mode === 'several/empty') return send(204, 'application/json');
      return send(404, 'text/plain', 'Unknown fixture');
    }
    if (request.url.startsWith('/json')) assert.equal(decodeURIComponent(request.url), "/json/a'b&c");
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ method: request.method, body: Buffer.concat(chunks).toString('hex'), contentType: request.headers['content-type'] }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  for (const [suffix, fetcher] of [['', undefined], ['', crossFetch], ['-import', undefined]]) {
  const { MediaClient } = require(join(temporary, `compiled/native-sdk${suffix}.js`));
  const client = new MediaClient(`http://127.0.0.1:${server.address().port}`, fetcher);
  const { createForkLaunchSdk } = require(join(temporary, `compiled/typed-native-sdk${suffix}.js`));
  const typed = createForkLaunchSdk({ baseUrl: `http://127.0.0.1:${server.address().port}`, fetch: fetcher }).media;
  for (const [method, body, expected, contentType] of [
    ['postJson', { user: '水', active: true }, Buffer.from(JSON.stringify({ user: '水', active: true })), 'application/vnd.forklaunch+json; profile="https://example.test/v1"'],
    ['postText', 'hello 水', Buffer.from('hello 水'), 'text/plain; charset=utf-8'],
    ['postText', '', Buffer.alloc(0), 'text/plain; charset=utf-8'],
    ['postUrlencoded', new URLSearchParams(), Buffer.alloc(0), 'application/x-www-form-urlencoded'],
    ['postBinary', new Uint8Array([0, 255, 13]).buffer, Buffer.from([0, 255, 13]), 'application/octet-stream'],
    ['postBinary', new Uint8Array([99, 0, 255, 1, 99]).subarray(1, 4), Buffer.from([0, 255, 1]), 'application/octet-stream'],
    ['postFile', new Blob([new Uint8Array([137, 80, 78, 71])]), Buffer.from([137, 80, 78, 71]), 'image/png'],
    ['postUrlencoded', new URLSearchParams([['key', 'a b'], ['key', 'c/d']]), Buffer.from('key=a+b&key=c%2Fd'), 'application/x-www-form-urlencoded']
  ]) {
    const echoed = await (await client[method]({ body })).json();
    assert.equal(echoed.method, 'POST');
    assert.equal(echoed.body, expected.toString('hex'));
    assert.equal(echoed.contentType, contentType);
    const router = method.slice('post'.length).toLowerCase();
    const reply = await typed[router][method]({ body });
    assert.equal(reply.code, 200);
    assert.deepEqual(reply.response, echoed);
  }
  const form = new FormData();
  form.append('label', 'hello 水');
  form.append('label', 'repeat');
  form.append('file', new Blob([new Uint8Array([0, 255, 1])]), 'data.bin');
  form.append('file', new Blob([]), 'empty.bin');
  const echoed = await (await client.postForm({ body: form, headers: { 'Content-Type': 'multipart/form-data; boundary=incorrect' } })).json();
  assert.match(echoed.contentType, /^multipart\/form-data; boundary=.+/);
  const bytes = Buffer.from(echoed.body, 'hex');
  assert.ok(bytes.includes(Buffer.from('hello 水')));
  assert.ok(bytes.includes(Buffer.from([0, 255, 1])));
  assert.ok(bytes.includes(Buffer.from('filename="data.bin"')));
  for (const body of [form, { label: ['hello 水', 'repeat'], file: [new Blob([new Uint8Array([0, 255, 1])]), new Blob([])] }, {}]) {
    const reply = await typed.form.postForm({ body, headers: { 'Content-Type': 'multipart/form-data; boundary=incorrect' } });
    assert.equal(reply.code, 200);
    const parsed = await new Response(Buffer.from(reply.response.body, 'hex'), { headers: { 'content-type': reply.response.contentType } }).formData();
    if (Object.keys(body).length || body instanceof FormData) {
      assert.deepEqual(parsed.getAll('label'), ['hello 水', 'repeat']);
      assert.equal(parsed.getAll('file').length, 2);
      assert.equal((await parsed.getAll('file')[1].arrayBuffer()).byteLength, 0);
      assert.deepEqual([...new Uint8Array(await parsed.get('file').arrayBuffer())], [0, 255, 1]);
    } else assert.equal([...parsed].length, 0);
  }
  // Generated clients encode form collections with bracket keys and refuse empty
  // ones, which have no lossless wire form (fb1e03d21).
  const urlencoded = await typed.urlencoded.postUrlencoded({ body: { key: ['a', '水'], number: 42 } });
  const encoded = new URLSearchParams(Buffer.from(urlencoded.response.body, 'hex').toString());
  assert.deepEqual([...encoded], [['key[0]', 'a'], ['key[1]', '水'], ['number', '42']]);
  await assert.rejects(typed.urlencoded.postUrlencoded({ body: { empty: [] } }), /Empty form collection has no lossless wire representation/);
  assert.equal((await typed.responses.readText()).response, 'hello 水\n');
  assert.deepEqual([...(await typed.responses.readBinary()).response], [0, 255, 13, 10]);
  assert.equal((await typed.responses.readNull()).response, null);
  assert.equal((await typed.responses.readEmpty()).response, undefined);
  assert.equal((await typed.responses.readHead()).response, undefined);
  const businessError = await typed.responses.readError();
  assert.equal(businessError.code, 409);
  assert.equal(businessError.contentType, 'application/problem+json');
  assert.deepEqual(businessError.response, { errorCode: 'CONFLICT' });
  for (const [mode, code, value] of [['created', 201, { createdAt: 'today' }], ['queued', 202, { jobId: 42 }], ['empty', 204, undefined]]) {
    const reply = await typed.responses.readSeveral({ params: { mode } });
    assert.equal(reply.code, code);
    assert.deepEqual(reply.response, value);
  }
  await assert.rejects(typed.responses.readInvalidJson(), SyntaxError);
  console.log(`PASS native SDK transport: ${suffix ? 'imported cross-fetch' : fetcher ? 'injected cross-fetch' : 'native fetch'}`);
  }
  console.log('PASS framework SDKs: strict TypeScript and Rust compilation, raw text/binary/file bytes, vendor JSON media, real multipart boundaries and URL-encoded uploads');
} finally {
  if (server) {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
  rmSync(temporary, { recursive: true, force: true });
}

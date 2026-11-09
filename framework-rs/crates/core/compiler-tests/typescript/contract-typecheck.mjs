// Typecheck the actual generated client, including correlated status/media narrowing.
import { readFileSync, writeFileSync, mkdtempSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
const require = createRequire(import.meta.url);
const compiler = process.env.FORKLAUNCH_TYPESCRIPT_COMPILER ?? require.resolve('typescript/bin/tsc');
const directory = mkdtempSync(join(tmpdir(), 'forklaunch-core-types-'));
try {
  symlinkSync(process.env.FORKLAUNCH_SDK_NODE_MODULES ?? dirname(dirname(dirname(compiler))), join(directory, 'node_modules'), 'dir');
  writeFileSync(join(directory, 'package.json'), '{"type":"module"}');
  const source = readFileSync(new URL('../../src/typescript/snapshots/catalog.typescript.snap', import.meta.url), 'utf8');
  const filename = join(directory, 'client.ts');
  writeFileSync(filename, source + `
async function checkDiscrimination() {
 const sdk = createForkLaunchSdk({ baseUrl: 'https://example.test' });
 const result = await sdk.catalog.items.getItem({ params: { id: 'one' }, query: { includeDetails: true }, headers: { 'x-request-id': 'r', 'x-revision': 2 } });
 if (result.status === 200 && result.contentType === 'application/json') { const id: string = result.response.id; const count: number = result.headers['x-count']; const next: string | undefined = result.headers['x-next']; void id; void count; void next; }
 if (result.status === 200 && result.contentType === 'text/html') { const html: string = result.response; void html; }
 if (result.status === 302) { const empty: undefined = result.response; void empty; }
 if (result.status === 200 && result.contentType === 'application/pdf') { const bytes: Uint8Array = result.response; void bytes; }
 if (result.status === 200 && result.contentType === 'text/csv') { const csv: string = result.response; void csv; }
 if (result.status === 200 && result.contentType === 'text/plain') { const text: string = result.response; void text; }
 // @ts-expect-error schema-typed query forbids string values
 await sdk.catalog.items.getItem({ params: { id: 'one' }, query: { includeDetails: 'yes' } });
 // @ts-expect-error schema-typed header forbids numeric request identifiers
 await sdk.catalog.items.getItem({ params: { id: 'one' }, query: {}, headers: { 'x-request-id': 2 } });
}
`);
  execFileSync(process.execPath, [compiler, '--noEmit', '--strict', '--skipLibCheck', '--target', 'ES2022', '--module', 'NodeNext', '--moduleResolution', 'NodeNext', '--lib', 'ES2022,DOM,DOM.Iterable', filename], { stdio: 'inherit' });
  console.log('PASS generated TypeScript schema fields and correlated status/content-type narrowing');
} finally { rmSync(directory, { recursive: true, force: true }); }

// The CLI TypeScript SDK runtime (request/transport, no operations), rendered the
// way cli/src/sdk/generate.rs renders sdk/typescript/index.ts: the framework
// schema runtime (forklaunch_core typescript_schema_runtime_source) precedes
// transport, the SDK form stringifier and SSE helper are inlined, and mustache
// section markers are dropped.
import { readFile, realpath, symlink } from 'node:fs/promises';
import path from 'node:path';

export async function cliTypescriptSdkRuntime(root) {
  const read = (relative) => readFile(path.join(root, relative), 'utf8');
  const runtimeDir = 'framework-rs/crates/core/src/typescript';
  const schemaRuntime =
    (await read(`${runtimeDir}/schema-runtime.ts`)) +
    (await read(`${runtimeDir}/date-codec.ts`)) +
    (await read(`${runtimeDir}/zod-checks.ts`)).replace("import type Ajv from 'ajv';", '') +
    (await read(`${runtimeDir}/zod-regex.ts`)).replace("import type Ajv from 'ajv';", '') +
    `const socketDraft6MetaSchema = JSON.parse(${JSON.stringify(await read(`${runtimeDir}/draft6-schema.json`))}) as Record<string, unknown>;\n`;
  const transport = await read('cli/src/templates/sdk/typescript/transport.ts');
  const form = await read('cli/src/templates/sdk/typescript/safeStringify.ts');
  const sse = await read('cli/src/templates/sdk/typescript/sse.ts');
  const template = (await read('cli/src/templates/sdk/typescript/index.ts'))
    .replace('{{{transport_source}}}', () => `${schemaRuntime}\n${transport}`)
    .replace('{{{form_stringify_source}}}', () => form)
    .replace('{{{sse_source}}}', () => sse);
  const runtime =
    template.slice(0, template.indexOf('{{#operation_types}}')) +
    template.slice(template.indexOf('function pathWithParams'), template.indexOf('export function createForkLaunchSdk')) +
    '\nexport { request };\n';
  return runtime.replace(/\{\{![^}]*\}\}/g, '');
}

/** Resolve the runtime's Ajv imports from the pinned TypeScript fixture prefix. */
export async function linkTypescriptPrefix(tsc, directory) {
  // <prefix>/node_modules/.bin/tsc -> <prefix>/node_modules/typescript/bin/tsc
  const prefix = path.dirname(path.dirname(path.dirname(path.dirname(await realpath(tsc)))));
  await symlink(path.join(prefix, 'node_modules'), path.join(directory, 'node_modules'), 'dir');
}

export const cliSdkCompilerOptions = {
  strict: true,
  target: 'ES2022',
  module: 'CommonJS',
  lib: ['ES2022', 'DOM', 'DOM.Iterable'],
  types: [],
  outDir: 'compiled'
};

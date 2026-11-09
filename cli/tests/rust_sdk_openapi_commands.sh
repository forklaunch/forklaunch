#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
export FORKLAUNCH_CLI="${FORKLAUNCH_CLI:-$REPO_ROOT/cli/target/debug/forklaunch}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/cli/target}"
export CARGO_INCREMENTAL=0
export FORKLAUNCH_SKIP_FORMAT=1
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
if [ "${FORKLAUNCH_WORKER_RUNTIME_LOCKED:-0}" != 1 ]; then
    exec python3 "$SCRIPT_DIR/fixtures/worker_runtime_lock.py" "$SCRIPT_DIR/rust_sdk_openapi_commands.sh"
fi
python3 - "$REPO_ROOT" <<'PY'
"""Exercise real project-local SDKs, artifact imports and compiler freshness."""
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import tomllib

repo = Path(sys.argv[1])
source_cli = Path(os.environ['FORKLAUNCH_CLI']).resolve()
root = Path(tempfile.mkdtemp(prefix='forklaunch-sdk-openapi-'))
cli = root / 'forklaunch'
shutil.copy2(source_cli, cli)
print('CLI SHA-256:', hashlib.sha256(cli.read_bytes()).hexdigest(), flush=True)
app = root / 'app'
env = dict(os.environ, FORKLAUNCH_CLI=str(cli))

def run(*args, cwd=None, expected=0):
    print('Running:', shlex.join(map(str, args)), flush=True)
    result = subprocess.run(list(map(str, args)), cwd=cwd or (app if app.exists() else repo), env=env,
                            capture_output=expected != 0, text=True)
    if expected == 0:
        assert result.returncode == 0, args
    else:
        assert result.returncode != 0, f'Command should fail: {args}'
        print(result.stdout + result.stderr, flush=True)
    return result

def command(*args, path=None, expected=0):
    return run(cli, *args, '--path', path or app, expected=expected)

def generated(name, target='typescript'):
    return app / 'src' / name / 'sdk' / target

def digest(directory):
    return {str(path.relative_to(directory)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in directory.rglob('*') if path.is_file() and 'node_modules' not in path.parts}

try:
    command('init', 'application', 'rust-contracts', '--database', 'postgresql', '--validator', 'zod',
            '--formatter', 'prettier', '--linter', 'oxlint', '--http-framework', 'express', '--runtime', 'node',
            '--test-framework', 'vitest', '-D', 'SDK command fixture', '-A', 'ForkLaunch', '-L', 'MIT')
    for service, router in [('accounts', 'users'), ('billing', 'invoices')]:
        command('init', 'service', service, '--language', 'rust', '-D', service)
        command('init', 'router', router, '--language', 'rust', path=app / 'src' / service)
        assert (app / 'src' / service / '.forklaunch/contracts/Cargo.toml').is_file()
        assert '#[forklaunch_core::contract::api]' in (app / 'src' / service / 'api/controllers' / (router+'.rs')).read_text()
    # cargo check itself must emit both languages' SDKs and OpenAPI.
    run('cargo', 'check', '--workspace', '--all-targets')
    for service, router in [('accounts', 'users'), ('billing', 'invoices')]:
        sdk = generated(service)
        spec = json.loads((sdk / 'openapi.json').read_text())
        contract = json.loads((sdk / 'contract.json').read_text())
        assert spec['openapi'] == '3.1.0' and '/'+router in spec['paths']
        assert contract['formatVersion'] == 'forklaunch.contract.v1'
        assert contract['generator'] == 'framework-rs'
        assert contract['sdkLayout'] == 'project-local-v1'
        assert (sdk / 'index.ts').read_text().strip() == "export type * from './sdk.js';"
        text = (sdk / 'sdk.ts').read_text()
        assert 'import type {' in text and "from '@forklaunch/universal-sdk'" in text
        assert '& Record<string, unknown>' not in text
        assert 'export type '+service.title()+'SdkClient' in text
        assert (generated(service, 'rust') / 'generated/client.rs').is_file()
        assert 'pub struct Client' in (generated(service, 'rust') / 'generated/client.rs').read_text()
        assert tomllib.loads((app / 'src' / service / '.forklaunch/contracts/Cargo.toml').read_text())['package']['publish'] is False
    assert not (app / 'sdks').exists()
    command('openapi', 'export')
    # Default exports refresh each service's SDK folder; .forklaunch/ keeps only the manifest.
    assert sorted(str(p.relative_to(app)) for p in (app / '.forklaunch').rglob('*')) == ['.forklaunch/manifest.toml']
    command('openapi', 'export', '--output', 'generated/openapi')
    for service in ['accounts', 'billing']:
        assert (app / 'generated/openapi' / service / 'openapi.json').is_file()
        for artifact in [generated(service) / 'openapi.json', app / 'generated/openapi' / service / 'openapi.json']:
            exported = json.loads(artifact.read_text())
            assert all('x-forklaunch-operation' in item['get'] for item in exported['paths'].values()), 'Export must retain checked portable SDK operation descriptors'
    command('sdk', 'generate', 'all', '--from', 'openapi')
    command('sdk', 'generate', '--target', 'all', '--from', 'openapi', '--check')
    # Compile against the real bundled TypeScript runtime in an isolated install.
    runtime = root / 'typescript-runtime'
    import sys; sys.path.insert(0, str(repo / 'cli/tests/fixtures')); from universal_sdk_source import copy_universal_sdk
    copy_universal_sdk(repo, runtime)
    run('npm', 'install', '--prefix', runtime, '--ignore-scripts', '--package-lock=false')
    run('npm', '--prefix', runtime, 'run', 'build')
    run('node', repo / 'framework/universal-sdk/__test__/publicTransport.mjs', runtime)
    dependency = app / 'node_modules/@forklaunch'
    dependency.mkdir(parents=True)
    (dependency / 'universal-sdk').symlink_to(runtime, target_is_directory=True)
    tsc = Path(os.environ.get('TSC_BIN', runtime / 'node_modules/.bin/tsc'))
    for service in ['accounts', 'billing']:
        run(tsc, '--project', generated(service) / 'tsconfig.json')
    usage = app / 'sdk-consumer.ts'
    usage.write_text('''import type { AccountsSdkClient, ListUsersRequest, ListUsersResponse, UsersResponse, RecordModel as UserRecord } from './src/accounts/sdk/typescript/index.js';
import type { BillingSdkClient } from './src/billing/sdk/typescript/index.js';
declare const accounts: AccountsSdkClient;
declare const billing: BillingSdkClient;
const record: UserRecord = { id: 'one', name: 'Ada' };
const input: ListUsersRequest = { signal: new AbortController().signal, credentials: 'include' };
const response: Promise<ListUsersResponse> = accounts.users.listUsers(input);
void billing.invoices.listInvoices({ cache: 'no-store' });
async function narrow() {
  const result = await response;
  if (result.code === 200) {
    const json: 'application/json' = result.contentType;
    const body: UsersResponse = result.response;
    const name: string = body.records[0].name;
    return [json, name];
  }
}
// @ts-expect-error Known record fields remain strongly typed.
const invalidName: UserRecord = { id: 'one', name: 42 };
// @ts-expect-error Unspecified properties must not be widened by Record<string, unknown>.
const extra: UserRecord = { id: 'one', name: 'Ada', invented: true };
// @ts-expect-error A GET endpoint has no request body.
accounts.users.listUsers({ body: { unexpected: true } });
// @ts-expect-error Removed or unknown routes are not callable.
accounts.users.missing({});
void [record, invalidName, extra, narrow];
''')
    consumer_config = app / 'sdk-consumer.tsconfig.json'
    consumer_config.write_text(json.dumps({'compilerOptions': {'noEmit': True, 'strict': True,
        'target': 'ES2022', 'module': 'ESNext', 'moduleResolution': 'Bundler', 'lib': ['ES2022', 'DOM']},
        'files': ['./sdk-consumer.ts']}))
    run(tsc, '--project', consumer_config)
    # The service SDK's transport is the shared runtime. Exercise its public
    # TypeScript entry point against a real emitted operation descriptor.
    path_check = root / 'path-conformance.mjs'
    path_check.write_text('''import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
const { universalSdk } = await import(pathToFileURL(process.argv[2]));
const document = JSON.parse(await readFile(process.argv[3], 'utf8'));
const operation = document.paths['/users'].get;
const path = '/users/:id/:id2/{other}/literal:id/{missing}';
operation['x-forklaunch-operation'].path = path;
document.paths = { [path]: { get: operation } };
let captured;
globalThis.fetch = async url => {
  captured = String(url);
  return new Response(JSON.stringify({ route: 'users', summary: { service: 'accounts', total: 0 }, records: [] }), { headers: { 'content-type': 'application/json' } });
};
const client = await universalSdk({ host: 'https://fixture.invalid', registryOptions: { raw: document } });
await client.users.listUsers({ params: { id: 'a/b ?#%é🦀', id2: 'second', other: ':id2/{id}', literal: 'ignored' } });
assert.equal(captured, 'https://fixture.invalid/users/a%2Fb%20%3F%23%25%C3%A9%F0%9F%A6%80/second/%3Aid2%2F%7Bid%7D/literal:id/{missing}');
console.log('PASS shared TypeScript SDK runtime path parameter conformance');
''')
    run('node', path_check, runtime / 'lib/index.mjs', generated('accounts') / 'openapi.json')
    rust_tests = generated('accounts', 'rust') / 'tests'
    rust_tests.mkdir()
    (rust_tests / 'path_conformance.rs').write_text((repo / 'cli/tests/fixtures/rust_sdk_paths.rs').read_text().replace('use super::path_with_params;', 'use forklaunch_universal_sdk::path_with_params;'))
    run('cargo', 'test', '-p', 'accounts-sdk', '--test', 'path_conformance')
    # Explicit OpenAPI generation reads the edited artifact, never reconstructed routes.
    artifact = generated('accounts') / 'openapi.json'
    original = artifact.read_bytes()
    spec = json.loads(original)
    spec['paths']['/users']['get']['operationId'] = 'scanUsers'
    response_schema = spec['paths']['/users']['get']['responses']['200']['content']['application/json']['schema']
    if '$ref' in response_schema:
        response_schema = spec['components']['schemas'][response_schema['$ref'].rsplit('/', 1)[1]]
    response_schema['properties']['displayName'] = {'type': 'string'}
    artifact.write_text(json.dumps(spec))
    command('sdk', 'generate', '--target', 'all', '--from', 'openapi', '--check', expected=1)
    command('sdk', 'generate', '--target', 'all', '--from', 'openapi')
    assert 'scanUsers:' in (generated('accounts') / 'sdk.ts').read_text()
    assert 'displayName?: string' in (generated('accounts') / 'sdk.ts').read_text()
    assert 'pub display_name:' in (generated('accounts', 'rust') / 'generated/models.rs').read_text()
    artifact.unlink()
    command('sdk', 'generate', '--target', 'all', '--from', 'openapi', expected=1)
    artifact.write_bytes(original)
    command('sdk', 'generate', '--target', 'all', '--from', 'contract')
    command('sdk', 'generate', 'rust', '--target', 'typescript', '--from', 'contract', expected=1)
    billing_before = digest(generated('billing'))
    command('sdk', 'generate', 'ts', '--service', 'accounts', '--from', 'contract')
    assert digest(generated('billing')) == billing_before, 'Filtered generation must preserve other service SDKs'
    sdk = generated('accounts') / 'sdk.ts'
    sdk.write_text(sdk.read_text()+'\n// deliberately stale generated SDK\n')
    command('sdk', 'generate', '--target', 'typescript', '--service', 'accounts', '--from', 'contract', '--check', expected=1)
    command('sdk', 'watch', '--target', 'typescript', '--service', 'accounts', '--from', 'contract', '--once', '--interval-ms', '100')
    command('sdk', 'generate', '--target', 'all', '--from', 'contract', '--check')
    command('sdk', 'generate', 'rust', '--service', 'billing', '--from', 'contract')
    command('change', 'service', '-N', 'invoicing', '--confirm', path=app / 'src/billing')
    assert not (app / 'src/billing').exists()
    assert 'InvoicingSdkClient' in (generated('invoicing') / 'sdk.ts').read_text()
    command('sdk', 'generate', '--target', 'all', '--from', 'contract', '--check')
    run('cargo', 'check', '--workspace', '--all-targets')
    run(tsc, '--project', generated('invoicing') / 'tsconfig.json')
    # Editing an inline contract must refresh live SDK typing on a subsequent
    # ordinary compile; no explicit sdk command is allowed between edit and check.
    controller = app / 'src/accounts/api/controllers/users.rs'
    original_controller = controller.read_text()
    assert 'List Users' in original_controller
    controller.write_text(original_controller.replace('List Users', 'Find Users'))
    run('cargo', 'check', '-p', 'accounts', '-p', 'accounts-sdk')
    refreshed = (generated('accounts') / 'sdk.ts').read_text()
    assert 'findUsers:' in refreshed and 'listUsers:' not in refreshed
    assert 'FindUsersResponse' in refreshed
    controller.write_text(original_controller)
    run('cargo', 'check', '-p', 'accounts', '-p', 'accounts-sdk')
    command('sdk', 'generate', '--target', 'all', '--from', 'contract', '--check')
    print('PASS project-local SDKs on compilation and contract edits, discrete TS types, Rust clients, OpenAPI edits, missing-artifact rejection, service filtering, watch freshness and rename', flush=True)
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print('Fixture retained:', root, flush=True)
    else:
        shutil.rmtree(root)
PY

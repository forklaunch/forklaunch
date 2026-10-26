#!/usr/bin/env python3
"""Compare generated presence/null semantics to the real TypeScript validator."""
import hashlib
from fixtures.legacy_sdk_manifest import write_legacy_sdk_manifest
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile

repo = Path(__file__).resolve().parents[2]
fixtures = repo / 'framework-rs/parity/fixtures'
root = Path(tempfile.mkdtemp(prefix='forklaunch-presence-sdk-')).resolve()
cli = root / 'forklaunch'
shutil.copy2(os.environ['FORKLAUNCH_CLI'], cli)
digest = hashlib.sha256(cli.read_bytes()).hexdigest()
print('CLI SHA256:', digest, flush=True)
app = root / 'original'
env = dict(os.environ)
passed = False
cases = []


def run(*args, cwd=None):
    print('Running:', shlex.join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd or app, env=env, check=True, timeout=1800)


def command(*args, path=None):
    run(cli, *args, '--path', path or app, cwd=root)


def typescript(consumer, output):
    config = root / 'presence-tsconfig.json'
    config.write_text(json.dumps({'compilerOptions': {
        'strict': True, 'skipLibCheck': True, 'target': 'ES2022', 'module': 'ESNext',
        'moduleResolution': 'Bundler', 'lib': ['ES2022', 'DOM', 'DOM.Iterable'],
        'types': ['node'], 'typeRoots': [str(shared / 'node_modules/@types')],
        'rootDir': str(consumer.parent), 'outDir': str(output), 'noEmitOnError': True,
    }, 'files': [str(consumer)]}))
    run(env['TSC_BIN'], '--project', config)


def rust_client(directory, modern=False, standalone=False):
    manifest = directory / 'Cargo.toml'
    source = manifest.read_text()
    if '[dev-dependencies]' not in source:
        source += '\n[dev-dependencies]\ntokio = {version="1",features=["macros","rt-multi-thread","time"]}\n'
    manifest.write_text(source)
    source = (fixtures / 'rust_sdk_presence.rs').read_text()
    if modern or standalone:
        prefix = '' if modern else 'Presence'
        source = source.replace('PresencePresence', prefix)
        source = source.replace('params: [("index".into(), index.to_string())].into(),',
                                f'params: {prefix}ReadPresenceParams {{index:index.to_string()}},')
        if modern:
            source = re.sub(r'\s*else \{\s*panic!\("[^"]*"\)\s*\}', '', source)
    if modern:
        source = source.replace('PresenceClient::', 'Client::').replace('.presence_read_presence(', '.read_presence(').replace('.presence_write_presence(', '.write_presence(')
    example = directory / 'examples/presence_roundtrip.rs'
    example.parent.mkdir(exist_ok=True)
    example.write_text(source)
    run('cargo', 'build', '--manifest-path', manifest, '--example', 'presence_roundtrip')
    run('cargo', 'clippy', '--manifest-path', manifest, '--all-targets', '--', '-D', 'warnings')
    return Path(env['CARGO_TARGET_DIR']) / 'debug/examples/presence_roundtrip'


def live(rust, consumer, label):
    run('node', repo / 'framework-rs/parity/rust_sdk_presence_runtime.mjs', binary, rust, consumer,
        service / 'api/controllers/presence_cases.json')
    cases.append({'client': label, 'passed': True})


try:
    command('init', 'application', 'presence-app', '--database', 'postgresql', '--validator', 'zod',
            '--formatter', 'prettier', '--linter', 'oxlint', '--http-framework', 'express', '--runtime', 'node',
            '--test-framework', 'vitest', '-D', 'Presence', '-A', 'ForkLaunch', '-L', 'MIT')
    command('init', 'service', 'presence', '--language', 'rust', '-D', 'Presence')
    relocated = root / 'relocated'
    app.rename(relocated)
    app = relocated
    service = app / 'src/presence'
    # Dependencies support the actual repository validator source. The schema
    # and expected cases are never duplicated as hand-maintained approximations.
    shared = root / 'universal-sdk'
    import sys; sys.path.insert(0, str(repo / 'framework-rs/parity/fixtures')); from universal_sdk_source import copy_universal_sdk
    copy_universal_sdk(repo, shared)
    validator_dependencies = json.loads((repo / 'framework/validator/package.json').read_text())['dependencies']
    run('npm', 'install', '--prefix', shared, '--ignore-scripts', '--package-lock=false', '--no-save',
        *[f'{name}@{validator_dependencies[name]}' for name in ['zod', 'ts-deepmerge']])
    run('npm', '--prefix', shared, 'run', 'build')
    env['TSC_BIN'] = os.environ.get('TSC_BIN', str(shared / 'node_modules/.bin/tsc'))
    env['FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES'] = str(shared)
    run('node', repo / 'framework-rs/parity/rust_sdk_presence_schema.mjs', app, service / 'api/controllers')
    fixture = (fixtures / 'rust_sdk_presence_service.rs').read_text()
    start, end = fixture.index('fn body()'), fixture.index('pub fn register(')
    (service / 'api/controllers/presence_contracts.rs').write_text(
        'use forklaunch_core::contract::{BodyContract,FieldContract,RouteContract,SchemaRef};\n' + fixture[start:end])
    (service / 'api/routes/presence_fixture.rs').write_text(
        'use axum::{Json,extract::Path};\nuse forklaunch_axum::ForkLaunchRouter;\nuse serde_json::{Value,json};\nuse contracts::presence_fixture::contracts;\n' + fixture[end:].replace('include_str!("presence_cases.json")', 'include_str!("../controllers/presence_cases.json")'))
    host = service / '.forklaunch/contracts/lib.rs'
    host.write_text('#[path="../../api/controllers/presence_contracts.rs"]\npub mod presence_fixture;\n' +
        host.read_text().replace('//!', '//').replace('pub fn registry()', 'pub fn native_registry()').replace('pub fn binding_manifest()', 'pub fn native_binding_manifest()') +
        '\npub fn registry()->forklaunch_core::contract::ContractRegistry{let mut registry=native_registry();for route in presence_fixture::contracts(){registry.register(route);}registry}\n' +
        'pub fn binding_manifest()->Result<Vec<forklaunch_core::contract::assembly::BindingDeclaration>,String>{let mut bindings=native_binding_manifest()?;for route in presence_fixture::contracts(){bindings.push(forklaunch_core::contract::assembly::BindingDeclaration::http("api/controllers/presence_contracts.rs","presence_fixture",&route.operation_name,&route)?);}Ok(bindings)}\n')
    routes = service / 'api/routes/mod.rs'
    routes.write_text('pub mod presence_fixture;\n' + routes.read_text().replace('ContractRouter::new(contracts::registry())', 'ContractRouter::new(contracts::native_registry())'))
    cargo = service / 'Cargo.toml'
    cargo.write_text(cargo.read_text().replace('"native-server"', '"native-server", "adapter"', 1))
    application = service / 'application.rs'
    application.write_text(application.read_text().replace('    Ok(router)', '''    let fixture = crate::api::routes::presence_fixture::register(forklaunch_axum::ForkLaunchRouter::new(
        forklaunch_core::ServiceInfo::new("presence", env!("CARGO_PKG_VERSION")),
    ).with_config(forklaunch_axum::RouterConfig {openapi_enabled:false,docs_enabled:false,..Default::default()})).into_router();
    Ok(router.merge(fixture))'''))
    run('cargo', 'build', '--workspace')
    command('openapi', 'export')
    sdk = service / 'sdk/typescript'
    assert (sdk / 'index.ts').read_text().strip() == "export type * from './sdk.js';"
    binary = Path(env['CARGO_TARGET_DIR']) / 'debug/presence'
    run('node', repo / 'framework-rs/parity/rust_mcp_service_startup.mjs', binary)
    dependency = app / 'node_modules/@forklaunch'
    dependency.mkdir(parents=True)
    (dependency / 'universal-sdk').symlink_to(shared, target_is_directory=True)
    original_ts = (fixtures / 'rust_sdk_presence.ts').read_text()
    native_ts = app / 'presence_fixture.ts'
    native_ts.write_text(original_ts.replace(original_ts.splitlines()[0],
        "import {universalSdk} from '@forklaunch/universal-sdk';\nimport {readFile} from 'node:fs/promises';\nimport type {PresenceSdkClient, WritePresenceRequest as PresencePresenceWritePresenceRequest} from './src/presence/sdk/typescript/index.js';").replace(
        'const sdk = createForkLaunchSdk({ baseUrl });', 'const sdk={presence:await universalSdk<PresenceSdkClient>({host:baseUrl,registryOptions:{raw:JSON.parse(await readFile(' + json.dumps(str(sdk / 'openapi.json')) + ",'utf8'))}})};"))
    for source in ['contract', 'openapi']:
        command('sdk', 'generate', '--target', 'all', '--from', source)
        command('sdk', 'generate', '--target', 'all', '--from', source, '--check')
        typescript(native_ts, app / 'native-dist')
        live(rust_client(service / 'sdk/rust', modern=True), app / 'native-dist/presence_fixture.js', 'current-' + source)
    compat = root / 'legacy-compiler'
    artifacts = compat / '.forklaunch/contracts/presence'
    artifacts.mkdir(parents=True)
    write_legacy_sdk_manifest(app, compat / '.forklaunch/manifest.toml', '''[[projects]]
name="presence"
type="Service"
description = "Legacy compiler compatibility fixture"
routers = []
[projects.metadata]
language="rust"
cratePath="crates/presence"
contractArtifactPath=".forklaunch/contracts/presence/contract.json"
openapiPath=".forklaunch/contracts/presence/openapi.json"
''')
    artifact = json.loads((sdk / 'contract.json').read_text())
    for key in ['sdkCompiler', 'sdkLayout', 'typescriptRuntimeLayout', 'rustClientName', 'rustOutputLayout']:
        artifact.pop(key, None)
    (artifacts / 'contract.json').write_text(json.dumps(artifact))
    document = json.loads((sdk / 'openapi.json').read_text())
    for path in document['paths'].values():
        for operation in path.values():
            if isinstance(operation, dict):
                operation.pop('x-forklaunch-operation', None)
                operation.pop('x-forklaunch-operations', None)
    (artifacts / 'openapi.json').write_text(json.dumps(document))
    (compat / 'package.json').write_text('{"name":"legacy-presence-fixture","type":"module","private":true}')
    for source in ['contract', 'openapi']:
        command('sdk', 'generate', '--target', 'all', '--from', source, path=compat)
        command('sdk', 'generate', '--target', 'all', '--from', source, '--check', path=compat)
        if source == 'contract':
            run('npm', 'install', '--prefix', compat / 'sdks/typescript', '--ignore-scripts', '--package-lock=false')
        consumer = compat / 'sdks/typescript/src/presence_fixture.ts'
        consumer.write_text(original_ts)
        typescript(consumer, compat / 'sdks/typescript/dist')
        live(rust_client(compat / 'sdks/rust/presence-sdk'), compat / 'sdks/typescript/dist/presence_fixture.js', 'legacy-' + source)
    schema = service / 'api/controllers/presence_schema.json'
    rust_dir = compat / 'sdks/rust/presence-sdk'
    run('cargo', 'run', '--manifest-path', repo / 'framework-rs/Cargo.toml', '-p', 'forklaunch-core', '--features', 'build,asyncapi-import', '--example', 'rust_sdk_presence_client', '--', schema, rust_dir / 'src/lib.rs')
    rust = rust_client(rust_dir, standalone=True)
    run('cargo', 'run', '--manifest-path', repo / 'framework-rs/Cargo.toml', '-p', 'forklaunch-core', '--features', 'build,asyncapi-import', '--example', 'typescript_presence_client', '--', schema, compat / 'sdks/typescript/src/index.ts')
    typescript(consumer, compat / 'sdks/typescript/dist')
    live(rust, compat / 'sdks/typescript/dist/presence_fixture.js', 'framework-compilers')
    if os.environ.get('FORKLAUNCH_PRESENCE_RECEIPT'):
        Path(os.environ['FORKLAUNCH_PRESENCE_RECEIPT']).write_text(json.dumps({
            'schemaVersion': 1, 'cliSha256': digest, 'passed': True, 'cases': cases,
            'validatorCases': len(json.loads((service / 'api/controllers/presence_cases.json').read_text())),
            'checks': ['actual TypeScript validator', 'current neutral raw scaffold', 'service-local SDK types',
                       'missing, nullable, nested and escaped members', 'real Rust and TypeScript HTTP',
                       'MCP calls', 'both typed SDKs and compiler APIs', 'graceful shutdown'],
        }, indent=2) + '\n')
    passed = True
finally:
    if passed and not os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        shutil.rmtree(root)
    else:
        print('Fixture retained:', root, flush=True)

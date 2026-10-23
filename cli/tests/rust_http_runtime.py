#!/usr/bin/env python3
"""Current generated HTTP app plus explicit legacy SDK compiler conformance.

The wire cases are shared unchanged with the historical gate. Programmatic
fixture routes deliberately serve malformed responses to test SDK rejection;
ordinary health/users controllers and process/security composition are generated.
"""
from fixtures import golden
from fixtures.legacy_sdk_manifest import write_legacy_sdk_manifest
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

repo = Path(sys.argv[1]).resolve()
fixtures = repo / 'cli/tests/fixtures'
root = Path(tempfile.mkdtemp(prefix='forklaunch-http-runtime-')).resolve()
cli = root / 'forklaunch'
shutil.copy2(os.environ['FORKLAUNCH_CLI'], cli)
digest = hashlib.sha256(cli.read_bytes()).hexdigest()
print('CLI SHA-256:', digest, flush=True)
app = root / 'original'
env = dict(os.environ)
passed = False


def run(*args, cwd=None):
    print('Running:', shlex.join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd or app, env=env, check=True, timeout=1800)


def command(*args, path=None):
    run(cli, *args, '--path', path or app, cwd=root)


def configure_ts(tsc, paths, output=None, type_roots=None):
    config = root / 'fixture-tsconfig.json'
    options = {'strict': True, 'skipLibCheck': True, 'target': 'ES2022',
               'module': 'ESNext', 'moduleResolution': 'Bundler', 'lib': ['ES2022', 'DOM', 'DOM.Iterable']}
    if output:
        options['outDir'] = str(output)
        options['rootDir'] = os.path.commonpath([str(Path(path).parent) for path in paths])
        options['noEmitOnError'] = True
    else:
        options['noEmit'] = True
    if type_roots:
        options.update(types=['node'], typeRoots=[str(type_roots)])
    config.write_text(json.dumps({'compilerOptions': options, 'files': list(map(str, paths))}))
    run(tsc, '--project', config)


def client_example(directory, source, name):
    manifest = directory / 'Cargo.toml'
    text = manifest.read_text()
    if '[dev-dependencies]' not in text:
        text += '\n[dev-dependencies]\ntokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }\n'
    manifest.write_text(text)
    (directory / 'examples').mkdir(exist_ok=True)
    shutil.copy2(source, directory / 'examples' / (name + '.rs'))
    run('cargo', 'build', '--manifest-path', manifest, '--example', name)
    return Path(env['CARGO_TARGET_DIR']) / 'debug/examples' / name


try:
    # TypeScript shell plus the in-memory Rust service `accounts`: the golden
    # 'rust-service' scaffold (fixtures/golden.py). This protocol fixture uses
    # the generated in-memory service. PostgreSQL persistence/encryption is
    # exercised by dedicated database/runtime gates.
    golden.app('rust-service', app, built=False)
    command('init', 'router', 'users', '--language', 'rust', path=app / 'src/accounts')
    relocated = root / 'relocated'
    shutil.copytree(app, relocated)
    shutil.rmtree(app)
    app = relocated
    service = app / 'src/accounts'
    # The historical cross-language health payload publishes both fixture routers.
    health_service = service / 'domain/services/health.rs'
    health_service.write_text(health_service.read_text().replace('routes: vec!["health".into()]', 'routes: vec!["health".into(), "users".into()]'))
    # Declare the historical query encoding probe in the native contract so its
    # typed client can send it without an arbitrary map escape hatch.
    users_schema = service / 'domain/schemas/users.rs'
    users_schema.write_text(users_schema.read_text() + '''
#[forklaunch_core::contract::schema]
pub struct UsersQuery { pub label: Option<String> }
''')
    users_controller = service / 'api/controllers/users.rs'
    users_controller.write_text(users_controller.read_text()
        .replace('State(state): State<AppState>)', 'State(state): State<AppState>, _request: forklaunch_core::contract::endpoint::RequestFields<(), crate::domain::schemas::users::UsersQuery, forklaunch_core::contract::endpoint::NoFields>)')
        .replace('Endpoint<NoBody, JsonBody<', 'Endpoint<forklaunch_core::contract::endpoint::Fields<NoBody, crate::domain::schemas::users::UsersQuery>, JsonBody<'))
    # Keep every historical transport schema/wire case. Split declaration-only
    # input from handlers so the private compile host never imports Axum/runtime.
    transport = (fixtures / 'rust_sdk_transport.rs').read_text()
    declarations = transport[:transport.index('pub fn register(')]
    declarations = declarations.replace('use forklaunch_core::http::{Body, FrameworkRequest, FrameworkResponse, LocalRouter};\n', '')
    helper = transport[transport.index('fn json_body('):transport.index('fn multipart_response(')]
    (service / 'api/controllers/transport_contracts.rs').write_text(declarations + helper)
    handlers = transport[transport.index('pub fn register('):transport.index('fn json_body(')]
    handlers += transport[transport.index('fn multipart_response('):]
    handlers = 'use forklaunch_core::http::{Body, FrameworkRequest, FrameworkResponse, LocalRouter};\nuse contracts::transport_fixture::contracts;\n' + handlers
    (service / 'api/routes/transport_fixture.rs').write_text(handlers)
    host = service / '.forklaunch/contracts/lib.rs'
    host.write_text('#[path="../../api/controllers/transport_contracts.rs"]\npub mod transport_fixture;\n' + host.read_text().replace('//!', '//').replace('pub fn registry()', 'pub fn native_registry()').replace('pub fn binding_manifest()', 'pub fn native_binding_manifest()') + '\npub fn registry()->forklaunch_core::contract::ContractRegistry { let mut registry=native_registry(); for route in transport_fixture::contracts() {registry.register(route);} registry }\npub fn binding_manifest()->Result<Vec<forklaunch_core::contract::assembly::BindingDeclaration>,String>{let mut declarations=native_binding_manifest()?;for route in transport_fixture::contracts(){declarations.push(forklaunch_core::contract::assembly::BindingDeclaration::http("api/controllers/transport_contracts.rs","transport_fixture", &route.operation_name, &route)?);}Ok(declarations)}\n')
    routes = service / 'api/routes/mod.rs'
    routes.write_text('pub mod transport_fixture;\n' + routes.read_text().replace('ContractRouter::new(contracts::registry())', 'ContractRouter::new(contracts::native_registry())'))
    # Programmatic adapter remains an explicitly supported framework route API;
    # generated startup, docs, origin security, telemetry and MCP stay unchanged.
    cargo = service / 'Cargo.toml'
    cargo.write_text(cargo.read_text().replace('"native-server"', '"native-server", "adapter"', 1))
    application = service / 'application.rs'
    text = application.read_text().replace('    Ok(router)', '''    let mut handlers = forklaunch_core::http::LocalRouter::new();
    crate::api::routes::transport_fixture::register(&mut handlers);
    let fixture = forklaunch_axum::ForkLaunchRouter::from_local_router(
        forklaunch_core::ServiceInfo::new("accounts", env!("CARGO_PKG_VERSION")),
        contracts::transport_fixture::contracts(), handlers,
    ).with_config(forklaunch_axum::RouterConfig {
        openapi_enabled: true, openapi_path: "/legacy-openapi.json".into(),
        docs_enabled: true, docs_path: "/legacy-docs".into(),
        ..Default::default()
    }).into_router();
    Ok(router.merge(fixture))''')
    application.write_text(text)
    # Applications build against upstream serde_json with no root patch.
    import tomllib
    assert 'patch' not in tomllib.loads((app / 'Cargo.toml').read_text())
    # Current build.rs emits the local SDK and contract directly on compilation.
    run('cargo', 'build', '--workspace')
    sdk = service / 'sdk/typescript'
    assert (sdk / 'index.ts').read_text().strip() == "export type * from './sdk.js';"
    assert not (app / 'sdks').exists()
    compiled = json.loads((sdk / 'contract.json').read_text())
    assert compiled['sdkLayout'] == 'project-local-v1'
    assert {route['operationName'] for route in compiled['routes']} == {
        'getHealth', 'listUsers', 'getMultipartResponse', 'deleteUser', 'getStatus',
        'getText', 'getBinary', 'getUnknown', 'getCollision', 'getShape', 'uploadText',
        'uploadBinary', 'uploadMultipart', 'echoJson', 'getUnion', 'getUnionList',
        'getNullable', 'getEmpty', 'headHealth', 'getNull',
    }
    command('openapi', 'export')
    assert all('x-forklaunch-operation' in operation for path in json.loads((sdk / 'openapi.json').read_text())['paths'].values() for method, operation in path.items() if method in {'get','post','delete','head'})
    binary = Path(env['CARGO_TARGET_DIR']) / 'debug/accounts'
    # Install the actual bundled public package, never a test reimplementation.
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
    cross_fetch = Path(os.environ.get('CROSS_FETCH_PATH', tsc.parent.parent / 'cross-fetch'))
    if not cross_fetch.is_dir():
        run('npm', 'install', '--prefix', runtime, '--ignore-scripts', '--package-lock=false', '--no-save', 'cross-fetch@4.1.0')
        cross_fetch = runtime / 'node_modules/cross-fetch'
    env['CROSS_FETCH_PATH'] = str(cross_fetch)
    configure_ts(tsc, [fixtures / 'typescript_sdk_server.mts'], root / 'wire', runtime / 'node_modules/@types')
    wire = root / 'wire/typescript_sdk_server.mjs'
    driver = root / 'native-sdk.mjs'
    driver.write_text('''import {readFile} from 'node:fs/promises';
import {universalSdk, forkLaunchQueryOptions, forkLaunchMutationOptions, ForkLaunchSdkHttpError} from './typescript-runtime/lib/index.mjs';
export {forkLaunchQueryOptions, forkLaunchMutationOptions, ForkLaunchSdkHttpError};
const document=JSON.parse(await readFile(new URL('./relocated/src/accounts/sdk/typescript/openapi.json',import.meta.url),'utf8'));
// Test-only service namespace aligns the same wire assertions across compiler generations.
export async function createForkLaunchSdk({baseUrl, ...options}={}) {
 return {accounts: await universalSdk({host:baseUrl,registryOptions:{raw:document},...options})};
}
''')
    native_usage = app / 'native-usage.ts'
    usage = (fixtures / 'rust_http_consumer.ts').read_text()
    usage = usage.replace("import { createForkLaunchSdk } from './sdks/typescript/src/index';", "import type { AccountsSdkClient } from './src/accounts/sdk/typescript/index.js';\ndeclare function createForkLaunchSdk(): {accounts: AccountsSdkClient};")
    # Both compiler paths share the same precise DTO type policy. The fixture
    # checks declared keys and explicit dictionaries; wire tests retain extras.
    native_usage.write_text(usage)
    for source in ['contract', 'openapi']:
        command('sdk', 'generate', 'all', '--from', source)
        configure_ts(tsc, [native_usage])
        rust = client_example(service / 'sdk/rust', fixtures / 'rust_sdk_http_native.rs', 'native_http_roundtrip')
        run('node', fixtures / 'rust_http_runtime.mjs', binary, rust, wire, driver)
        print('PASS current local SDK HTTP matrix from ' + source, flush=True)
    # Explicit historical compiler compatibility. This is an authored artifact
    # fixture, not an alternate scaffold mode. The server remains the current app.
    compat = root / 'legacy-compiler'
    (compat / '.forklaunch').mkdir(parents=True)
    write_legacy_sdk_manifest(app, compat / '.forklaunch/manifest.toml', '''[[projects]]
name = "accounts"
type = "Service"
description = "Legacy HTTP compiler compatibility fixture"
routers = []
[projects.metadata]
language = "rust"
cratePath = "crates/accounts"
contractArtifactPath = ".forklaunch/contracts/accounts/contract.json"
openapiPath = ".forklaunch/contracts/accounts/openapi.json"
''')
    artifacts = compat / '.forklaunch/contracts/accounts'
    artifacts.mkdir(parents=True)
    contract = json.loads((sdk / 'contract.json').read_text())
    for key in ['sdkCompiler','sdkLayout','typescriptRuntimeLayout','rustClientName','rustOutputLayout']:
        contract.pop(key, None)
    # Preserve the historical success-only health/list input profile. Retained
    # Rust clients reject unlisted status/media pairs, just like current clients;
    # declared error responses on the transport routes remain typed variants.
    for route in contract['routes']:
        if route['method'] == 'GET' and route['path'] in ['/health', '/users']:
            route['responses'] = [response for response in route['responses'] if response['statusCode'] == 200]
    (artifacts / 'contract.json').write_text(json.dumps(contract))
    legacy_openapi = json.loads((sdk / 'openapi.json').read_text())
    for path in ['/health', '/users']:
        operation = legacy_openapi['paths'][path]['get']
        operation['responses'] = {'200': operation['responses']['200']}
    for path in legacy_openapi['paths'].values():
        for operation in path.values():
            if isinstance(operation, dict):
                operation.pop('x-forklaunch-operation', None)
    (artifacts / 'openapi.json').write_text(json.dumps(legacy_openapi))
    (compat / 'package.json').write_text('{"name":"legacy-http-fixture","type":"module","private":true}')
    legacy_usage = compat / 'legacy-usage.ts'
    shutil.copy2(fixtures / 'rust_http_consumer.ts', legacy_usage)
    for source in ['contract', 'openapi']:
        command('sdk', 'generate', 'all', '--from', source, path=compat)
        if source == 'contract':
            run('npm', 'install', '--prefix', compat / 'sdks/typescript', '--ignore-scripts', '--package-lock=false')
        configure_ts(tsc, [legacy_usage])
        legacy_index = compat / 'sdks/typescript/src/index.ts'
        configure_ts(tsc, [legacy_index], compat / 'sdks/typescript/dist')
        rust = client_example(compat / 'sdks/rust/accounts-sdk', fixtures / 'rust_sdk_http.rs', 'legacy_http_roundtrip')
        run('node', fixtures / 'rust_http_runtime.mjs', binary, rust, wire, compat / 'sdks/typescript/dist/index.js')
        print('PASS explicit legacy SDK compiler HTTP matrix from ' + source, flush=True)
    # Retain the independent framework generator's multipart conformance gate.
    run('cargo', 'run', '--manifest-path', repo / 'framework-rs/Cargo.toml', '-p', 'forklaunch-core', '--features', 'build,asyncapi-import', '--example', 'typescript_multipart_clients', '--', compat)
    rust_dir = compat / 'sdks/rust/accounts-sdk'
    shutil.copy2(compat / 'native-multipart.rs', rust_dir / 'src/lib.rs')
    rust = client_example(rust_dir, fixtures / 'rust_sdk_multipart.rs', 'native_multipart')
    run('cargo', 'clippy', '--manifest-path', rust_dir / 'Cargo.toml', '--lib', '--example', 'native_multipart', '--', '-D', 'warnings')
    # The independent compiler emits at the fixture root, outside the generated
    # package. Resolve the actual installed SDK dependencies for those sources.
    (compat / 'node_modules').symlink_to(compat / 'sdks/typescript/node_modules', target_is_directory=True)
    configure_ts(tsc, [compat / 'native-multipart.ts'], compat / 'native-multipart-dist')
    run('node', repo / 'cli/tests/rust_multipart_response_native.mjs', binary, rust, wire, compat / 'native-multipart-dist/native-multipart.js')
    print('PASS current and explicit legacy HTTP SDK matrices plus independent multipart compiler', flush=True)
    if env.get('FORKLAUNCH_HTTP_RUNTIME_RECEIPT'):
        Path(env['FORKLAUNCH_HTTP_RUNTIME_RECEIPT']).write_text(json.dumps({
            'status': 'PASS', 'cliSha256': digest,
            'checks': ['raw neutral application and relocated memory service',
                       'current contract/OpenAPI TS and Rust clients',
                       'explicit legacy contract/OpenAPI compatibility clients',
                       'original content types, validation and security wire assertions',
                       'independent framework multipart compiler and strict clippy'],
            'documentation': 'Native HTML UI references the canonical OpenAPI document with the declared typed query; legacy adapter HTML remains covered.',
        }, indent=2) + '\n')
    passed = True
finally:
    if passed and not os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        shutil.rmtree(root)
    else:
        print('Fixture retained:', root, flush=True)

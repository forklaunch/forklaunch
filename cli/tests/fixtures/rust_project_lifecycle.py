#!/usr/bin/env python3
"""Exercise current native project layout through raw init, sync and delete commands."""
import argparse
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
import tomllib


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--phase', choices=['init', 'paths', 'sync', 'delete', 'worker-origin'], required=True)
parser.add_argument('--skip-cargo', action='store_true', help='Probe CLI mutations only; CI must compile.')
args = parser.parse_args()
repo = Path(__file__).resolve().parents[3]
cli = Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch')).resolve()
env = {**os.environ, 'CARGO_INCREMENTAL': '0', 'FORKLAUNCH_SKIP_FORMAT': '1',
       'RUSTUP_TOOLCHAIN': 'stable'}
env.setdefault('CARGO_TARGET_DIR', str(repo / 'cli/target'))
if not env.get('FORKLAUNCH_WORKER_RUNTIME_LOCKED'):
    sys.exit(subprocess.run([
        sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'), '-c',
        shlex.join([sys.executable, str(Path(__file__).resolve()), *sys.argv[1:]]),
    ], env=env).returncode)

if not cli.is_file() and 'FORKLAUNCH_CLI' not in os.environ:
    subprocess.run(['cargo', 'build', '--manifest-path', str(repo / 'cli/Cargo.toml'), '--release'],
                   cwd=repo, env=env, check=True)
    cli = Path(env['CARGO_TARGET_DIR']) / 'release/forklaunch'

with cli.open('rb') as executable:
    print('CLI SHA256:', hashlib.file_digest(executable, 'sha256').hexdigest(), flush=True)

root = Path(tempfile.mkdtemp(prefix=f'forklaunch-project-{args.phase}-'))
app = root / 'app'

def rust_image():
    """The generated Rust image inventory, read from Dockerfile.rust and Compose."""
    lines = (app / 'Dockerfile.rust').read_text().splitlines()
    inputs = [json.loads(line[len('COPY '):])[0] for line in lines if line.startswith('COPY [')]
    release = 'COPY --from=builder /app/target/release/'
    binaries = [line[len(release):].split(' ')[0] for line in lines if line.startswith(release)]
    services = json.loads(subprocess.check_output(['docker', 'compose', '-f', str(app / 'docker-compose.yaml'),
        'config', '--no-interpolate', '--format', 'json'], cwd=app, text=True))['services']
    # This lifecycle asserts the migrate jobs explicitly alongside processes.
    processes = {name: service for name, service in services.items()
                 if (service.get('command') or [''])[0].startswith('/app/bin/')}
    return {'inputs': inputs, 'binaries': binaries, 'processes': processes}

modules = 'src'


def run(arguments, *, cwd=None, check=True):
    print('Running:', shlex.join(map(str, arguments)), flush=True)
    result = subprocess.run(arguments, cwd=cwd or (app if app.exists() else repo), env=env,
                            stdin=subprocess.DEVNULL, text=True, capture_output=True)
    if check:
        assert result.returncode == 0, result.stdout[-6000:] + result.stderr[-12000:]
    return result


def command(*arguments, path=None, check=True, cwd=None):
    return run([str(cli), *arguments, '--path', str(path or app)], check=check, cwd=cwd)


def manifest():
    return tomllib.loads((app / '.forklaunch/manifest.toml').read_text())


def cargo(directory):
    return tomllib.loads((directory / 'Cargo.toml').read_text())


def project(name, kind='service'):
    return app / modules / ('core' if kind == 'library' else '') / name


def snapshot(directory=None):
    directory = directory or app
    return {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in directory.rglob('*') if p.is_file() and '.git' not in p.relative_to(directory).parts}


def initialize(*, destination=None, modules_path='src', dryrun=False, cwd=None):
    options = ['--modules-path', modules_path]
    if dryrun:
        options.append('--dryrun')
    # Language and SDK targets belong to projects, not application initialization.
    command('init', 'application', 'lifecycle-proof', '--database', 'postgresql',
            '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint',
            '--http-framework', 'express', '--runtime', 'node', '--test-framework', 'vitest',
            '-D', 'Lifecycle proof', '-A', 'ForkLaunch', '-L', 'MIT', *options,
            path=destination or app, cwd=cwd)


def service(name='accounts', **kwargs):
    command('init', 'service', name, '--language', 'rust', '-D', name.title(), **kwargs)


def worker(name='jobs'):
    command('init', 'worker', name, '--language', 'rust', '--type', 'database',
            '--database', 'postgresql', '--runtime', 'apalis', '-D', name.title())


def library(name='shared'):
    command('init', 'library', name, '--language', 'rust', '-D', name.title())


def router(name, owner='accounts'):
    command('init', 'router', name, '--language', 'rust', path=project(owner))


def rust_projects():
    return {p['name']: p for p in manifest()['projects']
            if p.get('metadata', {}).get('language') == 'rust'}


def assert_registered(name, kind='service'):
    directory = project(name, kind)
    expected = str(directory.relative_to(app))
    metadata = rust_projects()[name]['metadata']
    assert metadata['cratePath'] == expected, metadata
    members = cargo(app)['workspace']['members']
    assert expected in members, members
    assert cargo(directory)['package']['name'] == name
    if kind != 'library':
        assert expected + '/.forklaunch/contracts' in members, members
        assert expected + '/sdk/rust' in members, members
        assert cargo(directory / '.forklaunch/contracts')['package']['name'] == name + '-contracts'
        assert cargo(directory / 'sdk/rust')['package']['name'] == name + '-sdk'
        assert metadata['sdkLayout'] == 'project-local-v1'
        assert metadata['contractArtifactPath'] == expected + '/sdk/typescript/contract.json'
    if kind == 'worker':
        assert metadata['workerService'] is True and metadata['workerRuntime'] == 'apalis'
        assert {entry['name'] for entry in cargo(directory)['bin']} == {name, name + '-worker'}


def add_custom_sources():
    with (project('shared', 'library') / 'lib.rs').open('a') as source:
        source.write('\npub fn custom_value() -> u32 { 42 }\n'
                     '#[cfg(test)] mod custom_test { #[test] fn retained() { '
                     'assert_eq!(super::custom_value(), 42); } }\n')
    with (project('accounts') / 'lib.rs').open('a') as source:
        source.write('\npub fn custom_dependency() -> u32 { shared::custom_value() }\n')
    for name in ['accounts', 'jobs']:
        with (project(name) / 'application.rs').open('a') as source:
            source.write(f'\n// Application-owned {name} composition must survive lifecycle operations.\n')


def assert_assets(name):
    directory = project(name)
    assert (directory / '.forklaunch/compliance.json').is_file()
    sdk = directory / 'sdk/typescript'
    assert json.loads((sdk / 'openapi.json').read_text())['openapi'] == '3.1.0'
    assert json.loads((sdk / 'contract.json').read_text())
    assert (sdk / 'index.ts').read_text() == "export type * from './sdk.js';\n"
    assert 'export type ' in (sdk / 'sdk.ts').read_text()
    assert 'dependencies' not in json.loads((sdk / 'package.json').read_text()), 'Service SDK exports only types'
    assert not (directory / 'types').exists(), 'Schemas are the public model source'


def compile_projects(*, test_library=True):
    if args.skip_cargo:
        print('PROBE ONLY: Cargo validation skipped; this is not a complete passing regression.', flush=True)
        return
    run(['cargo', 'check', '--workspace', '--all-targets'])
    if test_library and 'shared' in rust_projects():
        run(['cargo', 'test', '-p', 'shared'])
    for name, definition in rust_projects().items():
        if definition['type'] != 'Library':
            assert_assets(name)
    command('sdk', 'generate', '--target', 'all', '--from', 'contract', '--check')


def assert_removed(name, kind):
    relative = str(project(name, kind).relative_to(app))
    assert not project(name, kind).exists()
    assert name not in rust_projects()
    assert all(member != relative and not member.startswith(relative + '/')
               for member in cargo(app)['workspace']['members'])
    scripts = json.loads((app / 'package.json').read_text())['scripts']
    assert not any(key.endswith(':' + name) for key in scripts), scripts
    if (app / 'Dockerfile.rust').exists():
        processes = rust_image()['processes']
        assert name not in processes
        assert name + '-server' not in processes
        assert name + '-migrate' not in processes


def clone(source_name, destination_name, kind='service'):
    source = project(source_name, kind)
    destination = project(destination_name, kind)
    shutil.copytree(source, destination, ignore=shutil.ignore_patterns('target', 'node_modules'))
    for path in destination.rglob('*'):
        if path.is_file():
            text = path.read_text()
            # Change package identity, keeping ordinary domain modules such as
            # domain::schemas::jobs (and their source filenames) untouched.
            for suffix in ['', '-contracts', '-sdk', '-worker']:
                text = text.replace(f'"{source_name}{suffix}"', f'"{destination_name}{suffix}"')
            text = text.replace(f'/{source_name}"', f'/{destination_name}"')
            text = re.sub(r'(?<![\w:])' + re.escape(source_name.replace('-', '_')) + r'::',
                          destination_name.replace('-', '_') + '::', text)
            if path.name == 'package.json':
                package = json.loads(text)
                package['name'] = package['name'].replace('/' + source_name + '-', '/' + destination_name + '-')
                text = json.dumps(package, indent=2) + '\n'
            path.write_text(text)
    assert destination_name not in rust_projects()


def initial_projects():
    initialize()
    service()
    worker()
    library()
    router('users')
    add_custom_sources()


def assert_worker_origin(name, expected=None):
    config = json.loads(run(['docker', 'compose', '-f', str(app / 'docker-compose.yaml'),
                             'config', '--no-interpolate', '--format', 'json']).stdout)
    producer = config['services'][name + '-server']
    binding = next(port for port in producer['ports'] if port['target'] == 3000)
    published = int(binding['published'])
    assert binding['host_ip'] == '127.0.0.1'
    assert producer['environment']['PUBLIC_ORIGIN'] == (expected or f'http://localhost:{published}')
    return published


def check_worker_origin_lifecycle():
    path = app / 'docker-compose.yaml'
    original_port = assert_worker_origin('jobs')
    worker()
    assert assert_worker_origin('jobs') == original_port, 'Repeated init changed the published endpoint'
    # Reconcile only the old generated origin, using its actual published port.
    migrated_port = original_port + 10
    text = path.read_text().replace(f'http://localhost:{original_port}', 'http://jobs-server:3000')
    text = text.replace(f'127.0.0.1:{original_port}:3000', f'127.0.0.1:{migrated_port}:3000')
    path.write_text(text)
    command('sync', 'worker', 'jobs')
    assert assert_worker_origin('jobs') == migrated_port
    path.write_text(path.read_text().replace(f'http://localhost:{migrated_port}', 'http://jobs-server:3000'))
    command('change', 'worker', '-N', 'tasks', '--confirm', path=project('jobs'))
    assert assert_worker_origin('tasks') == migrated_port
    worker('tasks')
    assert assert_worker_origin('tasks') == migrated_port
    custom = 'https://jobs.example.test'
    path.write_text(path.read_text().replace(f'http://localhost:{migrated_port}', custom))
    command('sync', 'worker', 'tasks')
    worker('tasks')
    assert assert_worker_origin('tasks', custom) == migrated_port
    command('change', 'worker', '-N', 'jobs', '--confirm', path=project('tasks'))
    assert assert_worker_origin('jobs', custom) == migrated_port
    path.write_text(path.read_text().replace(custom, f'http://localhost:{migrated_port}'))
    print('PASS: worker producer localhost origin, stable ports, legacy sync/rename migration and custom origin preservation', flush=True)


def check_worker_origin():
    initialize()
    worker()
    check_worker_origin_lifecycle()
    compile_projects()


def check_init():
    initialize(destination=root / 'dry-app', dryrun=True)
    assert not (root / 'dry-app').exists()
    initial_projects()
    assert manifest()['modules_path'] == 'src'
    assert manifest()['language'] == 'mixed'
    assert (app / 'src/core/package.json').is_file(), 'TypeScript and Rust share the modules root'
    assert (app / 'src/client-sdk/package.json').is_file()
    assert not (app / 'crates').exists()
    for name, kind in [('accounts', 'service'), ('jobs', 'worker'), ('shared', 'library')]:
        assert_registered(name, kind)
        directory = project(name, kind)
        tasks = (directory / 'Makefile').read_text()
        for task in ['fmt:', 'lint:', 'check:', 'test:', 'build:']:
            assert task in tasks
        assert (directory / 'lib.rs').is_file()
    for name in ['accounts', 'jobs']:
        directory = project(name)
        for source in ['main.rs', 'application.rs', 'bootstrapper.rs', 'error.rs', 'build.rs',
                       'build_assets.rs', 'api/controllers/mod.rs', 'api/routes/mod.rs',
                       'domain/schemas/mod.rs', 'persistence/mod.rs']:
            assert (directory / source).is_file(), directory / source
        assert cargo(directory)['dependencies']['shared']['path'] == '../core/shared'
        assert (directory / 'migrations').is_dir()
    assert not (app / 'migrations').exists()
    assert {entry['name'] for entry in cargo(project('jobs'))['bin']} == {'jobs', 'jobs-worker'}
    assert (project('jobs') / 'consumer/worker_hooks.rs').is_file()
    assert '#[forklaunch_core::contract::api]' in (project('accounts') / 'api/controllers/users.rs').read_text()
    assert 'controller_contract!(users' in (project('accounts') / '.forklaunch/contracts/lib.rs').read_text()
    assert all((app / filename).is_file() for filename in ['Dockerfile.rust', 'Dockerfile.typescript'])
    # The PostgreSQL worker also gets its one-shot migration job from the shared image.
    assert set(rust_image()['processes']) == {'accounts', 'jobs', 'jobs-server', 'jobs-migrate'}, set(rust_image()['processes'])

    owned = {path: path.read_bytes() for name in ['accounts', 'jobs'] for path in project(name).rglob('*.rs')}
    service()
    router('users')
    assert all(path.read_bytes() == data for path, data in owned.items()), 'Repeated init replaced custom source'
    for arguments, path in [
        (['init', 'service', 'dry-service', '--language', 'rust', '-D', 'Dry service'], app),
        (['init', 'worker', 'dry-worker', '--language', 'rust', '-t', 'database', '-d', 'postgresql', '--runtime', 'apalis', '-D', 'Dry worker'], app),
        (['init', 'library', 'dry-library', '--language', 'rust', '-D', 'Dry library'], app),
        (['init', 'router', 'dry-router', '--language', 'rust'], project('accounts')),
    ]:
        before = snapshot()
        command(*arguments, '--dryrun', path=path)
        assert snapshot() == before, f'Dry run mutated the application: {arguments}'
    for name in [None, 'bad name']:
        before = snapshot()
        arguments = ['init', 'service', '--language', 'rust', '-D', 'Rejected name']
        if name is not None:
            arguments.append(name)
        result = command(*arguments, check=False)
        assert result.returncode != 0, result.stdout
        assert snapshot() == before, 'Invalid or missing name mutated the application'
    check_worker_origin_lifecycle()
    compile_projects()


def check_paths():
    global app, modules
    # Both supported alternative module roots keep Cargo at the application root.
    for modules in ['modules', 'src/modules']:
        app = root / modules.replace('/', '-')
        app.mkdir()
        initialize(destination='.', modules_path=modules, cwd=app)
        service(path=app / modules)
        library()
        router('users')
        assert manifest()['modules_path'] == modules
        assert_registered('accounts')
        assert_registered('shared', 'library')
        assert not (app / modules / 'Cargo.toml').exists()
        assert (app / modules / 'core/package.json').is_file()
        assert cargo(project('accounts'))['dependencies']['shared']['path'] == '../core/shared'
        for name in ['application.rs', 'api/controllers/users.rs', 'sdk/rust/lib.rs']:
            assert (project('accounts') / name).exists()
        clone('accounts', 'products')
        command('sync', 'all', '--confirm')
        assert_registered('products')
        command('init', 'library', 'clients', '--language', 'rust', '--sdk-services', 'accounts')
        aggregate = app / modules / 'client-sdk/rust'
        relative = str(aggregate.relative_to(app))
        assert rust_projects()['clients']['metadata']['cratePath'] == relative
        assert relative in cargo(app)['workspace']['members']
        aggregate_manifest = cargo(aggregate)
        # The aggregate re-exports every service-local SDK (e3842817d);
        # --sdk-services lists the ones needed now.
        assert aggregate_manifest['dependencies'] == {
            'accounts': {'package': 'accounts-sdk', 'path': '../../accounts/sdk/rust'},
            'products': {'package': 'products-sdk', 'path': '../../products/sdk/rust'},
        }, aggregate_manifest
        assert 'build-dependencies' not in aggregate_manifest
        assert not (aggregate / 'build.rs').exists(), 'Aggregate reexports existing SDKs without duplicate generation'
        public_exports = (aggregate / 'lib.rs').read_bytes()
        # With several services the aggregate re-exports each by name; a glob
        # re-export is only emitted for a single service.
        assert b'pub use accounts;' in public_exports and b'pub use products;' in public_exports
        assert b'::*;' not in public_exports.replace(b'pub use extensions::*;', b'')
        extension = aggregate / 'extensions.rs'
        extension.write_text('//! Application-owned SDK alias.\npub use crate::accounts::Client as AccountClient;\n')
        extension_source = extension.read_bytes()
        compile_projects()

        command('change', 'service', '-N', 'customer-accounts', '--confirm', path=project('accounts'))
        assert_registered('customer-accounts')
        assert not project('accounts').exists()
        assert cargo(aggregate)['dependencies'] == {
            'accounts': {'package': 'customer-accounts-sdk', 'path': '../../customer-accounts/sdk/rust'},
            'products': {'package': 'products-sdk', 'path': '../../products/sdk/rust'},
        }, cargo(aggregate)
        assert (aggregate / 'lib.rs').read_bytes() == public_exports
        assert extension.read_bytes() == extension_source
        # Deleting a service drops its re-export instead of blocking (e3842817d).
        extension.write_text('//! Application-owned SDK extensions.\n')
        command('delete', 'service', 'products', '--continue')
        assert not project('products').exists()
        assert 'products' not in cargo(aggregate)['dependencies'], cargo(aggregate)
        assert b'pub use products' not in (aggregate / 'lib.rs').read_bytes()
        compile_projects()


def check_sync():
    initial_projects()
    owned = {path: path.read_bytes() for name in ['accounts', 'jobs'] for path in project(name).rglob('*.rs')}
    for kind, name in [('service', 'accounts'), ('worker', 'jobs'), ('library', 'shared')]:
        command('sync', kind, name)
    command('sync', 'all', '--confirm')
    assert all(path.read_bytes() == data for path, data in owned.items()), 'Sync replaced custom source'
    # Nor does it add sources init would not write (an empty domain/mappers/mod.rs).
    # Generated SDK and event bindings are sync's to write.
    added = {path for name in ['accounts', 'jobs'] for path in project(name).rglob('*.rs')
             if not {'sdk', 'events'} & set(path.relative_to(project(name)).parts)} - owned.keys()
    assert not added, f'Sync added sources: {sorted(map(str, added))}'

    for kind, source, destination in [('service', 'accounts', 'products'), ('worker', 'jobs', 'media'),
                                      ('library', 'shared', 'common')]:
        clone(source, destination, kind)
        command('sync', kind, destination)
        assert_registered(destination, kind)
        if kind == 'service':
            assert rust_projects()[destination]['routers'] == ['users'], 'Sync must discover existing router modules'
        command('sync', kind, destination)
        assert sum(p['name'] == destination for p in manifest()['projects']) == 1
    clone('accounts', 'catalog')
    clone('shared', 'utils', 'library')
    command('sync', 'all', '--confirm')
    assert_registered('catalog')
    assert_registered('utils', 'library')
    for name in ['accounts', 'products', 'catalog', 'jobs', 'media']:
        assert cargo(project(name))['dependencies']['common']['path'] == '../core/common'
        assert cargo(project(name))['dependencies']['utils']['path'] == '../core/utils'
    before = snapshot()
    missing = command('sync', 'service', 'missing-service', check=False)
    assert missing.returncode != 0 and 'missing-service' in missing.stdout + missing.stderr
    assert snapshot() == before, 'Failed sync mutated application sources'
    # Reconcile all absent members before any shared-image or Cargo validation;
    # deleting one while another orphan remains must not strand the workspace.
    for name in ['products', 'catalog']:
        shutil.rmtree(project(name))
    command('sync', 'all', '--confirm')
    for name in ['products', 'catalog']:
        assert_removed(name, 'service')
    compile_projects()


def check_delete():
    initial_projects()
    service('notifications')
    router('internal-users')
    unaffected = project('notifications') / 'api/controllers/health.rs'
    unaffected.write_text(unaffected.read_text().replace('"Get Health"', '"Get Notification Health"'))
    retained_source = unaffected.read_bytes()
    compile_projects()
    if not args.skip_cargo:
        assert 'Get Notification Health' in (project('notifications') / 'sdk/typescript/contract.json').read_text()
    command('delete', 'router', 'users', '--continue', path=project('accounts'))
    for relative in ['api/controllers/users.rs', 'api/routes/users.rs', 'domain/schemas/users.rs']:
        assert not (project('accounts') / relative).exists(), relative
    assert (project('accounts') / 'api/routes/internal_users.rs').exists()
    assert 'controller_contract!(users' not in (project('accounts') / '.forklaunch/contracts/lib.rs').read_text()
    assert rust_projects()['accounts']['routers'] == ['internal-users']
    assert unaffected.read_bytes() == retained_source
    if not args.skip_cargo:
        compile_projects()
        schema = json.loads((project('accounts') / 'sdk/typescript/openapi.json').read_text())
        assert '/users' not in schema['paths'] and '/internal-users' in schema['paths']
        assert 'Get Notification Health' in (project('notifications') / 'sdk/typescript/contract.json').read_text()
    for kind, name in [('worker', 'jobs'), ('service', 'notifications')]:
        command('delete', kind, name, '--continue')
        assert_removed(name, kind)
    # Remove the application-owned use before deleting its dependency.
    source = project('accounts') / 'lib.rs'
    source.write_text(source.read_text().replace('pub fn custom_dependency() -> u32 { shared::custom_value() }', ''))
    command('delete', 'library', 'shared', '--continue')
    assert_removed('shared', 'library')
    assert 'shared' not in cargo(project('accounts'))['dependencies']
    compile_projects(test_library=False)
    command('delete', 'service', 'accounts', '--continue')
    assert_removed('accounts', 'service')
    assert cargo(app)['workspace']['members'] == []
    assert not rust_projects()
    assert (app / modules / 'core/package.json').is_file(), 'Rust deletion must preserve TypeScript core'


try:
    {'init': check_init, 'paths': check_paths, 'sync': check_sync, 'delete': check_delete, 'worker-origin': check_worker_origin}[args.phase]()
    print(f'{"PROBE" if args.skip_cargo else "PASS"}: raw Rust {args.phase} lifecycle, current layout, owned artifacts and source preservation', flush=True)
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print('Fixture retained:', root, flush=True)
    else:
        shutil.rmtree(root)

#!/usr/bin/env python3
"""Exercise changes through raw CLI scaffolding, then compile and run the result."""
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

repo = Path(__file__).resolve().parents[3]
cli = Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch')).resolve()
env = {**os.environ, 'CARGO_INCREMENTAL': '0', 'FORKLAUNCH_SKIP_FORMAT': '1', 'RUSTUP_TOOLCHAIN': 'stable'}
env.setdefault('CARGO_TARGET_DIR', str(repo / 'cli/target'))
if not env.get('FORKLAUNCH_WORKER_RUNTIME_LOCKED'):
    sys.exit(subprocess.run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'), '-c', shlex.join([sys.executable, str(Path(__file__).resolve())])], env=env).returncode)
root = Path(tempfile.mkdtemp(prefix='forklaunch-change-preservation-'))
app = root / 'app'

def rust_image():
    """The generated Rust image inventory, read from Dockerfile.rust and Compose."""
    lines = (app / 'Dockerfile.rust').read_text().splitlines()
    inputs = [json.loads(line[len('COPY '):])[0] for line in lines if line.startswith('COPY [')]
    release = 'COPY --from=builder /app/target/release/'
    binaries = [line[len(release):].split(' ')[0] for line in lines if line.startswith(release)]
    services = json.loads(subprocess.check_output(['docker', 'compose', '-f', str(app / 'docker-compose.yaml'),
        'config', '--no-interpolate', '--format', 'json'], cwd=app, text=True))['services']
    # Long-running processes; one-shot migrate/seed jobs run the same binary
    # with --migrate/--seed and are not processes.
    processes = {name: service for name, service in services.items()
                 if (service.get('command') or [''])[0].startswith('/app/bin/')
                 and not set(service['command'][1:]) & {'--migrate', '--seed'}}
    return {'inputs': inputs, 'binaries': binaries, 'processes': processes}


def run(args, cwd=None, check=True):
    print('Running:', shlex.join(map(str, args)), flush=True)
    result = subprocess.run(args, cwd=cwd or (app if app.exists() else repo), env=env, text=True, capture_output=True)
    if check:
        assert result.returncode == 0, result.stdout[-5000:] + result.stderr[-9000:]
    return result

def command(*args, path=None, check=True):
    return run([str(cli), *args, '--path', str(path or app)], check=check)

def tree():
    return {str(p.relative_to(app)): hashlib.sha256(p.read_bytes()).hexdigest() for p in app.rglob('*') if p.is_file()}

def cargo(path):
    return tomllib.loads(path.read_text())

def rejected(*args, path, message):
    before = tree()
    result = command(*args, '--confirm', path=path, check=False)
    assert result.returncode != 0 and message in result.stderr + result.stdout, result.stderr + result.stdout
    assert tree() == before, 'Rejected change mutated the application'

try:
    # Applications stay language-neutral; language belongs to individual projects.
    command('init', 'application', 'change-proof', '--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint', '--http-framework', 'express', '--runtime', 'node', '--test-framework', 'vitest', '-D', 'Changes', '-A', 'ForkLaunch', '-L', 'MIT')
    command('init', 'service', 'accounts', '--language', 'rust', '-D', 'Accounts')
    command('init', 'worker', 'jobs', '--language', 'rust', '-t', 'database', '-d', 'postgresql', '--runtime', 'apalis', '-D', 'Original worker')
    command('init', 'library', 'shared', '--language', 'rust', '-D', 'Original library')
    library = app / 'src/core/shared'
    service = app / 'src/accounts'
    worker = app / 'src/jobs'
    with (library / 'lib.rs').open('a') as f:
        f.write('\npub fn custom_value() -> u32 { 42 }\n#[cfg(test)] mod application_test { #[test] fn custom_export_survives() { assert_eq!(super::custom_value(),42); } }\n')
    with (service / 'lib.rs').open('a') as f:
        f.write('\npub fn custom_dependency() -> u32 { shared::custom_value() }\n')
    command('init', 'router', 'users', '--language', 'rust', path=service)
    command('change', 'application', '-N', 'changed-app', '-D', 'Changed application', '-A', 'ForkLaunch Team', '-L', 'AGPL-3.0', '--confirm')
    manifest = cargo(app / '.forklaunch/manifest.toml')
    assert manifest['app_name'] == 'changed-app' and manifest['app_description'] == 'Changed application'
    assert manifest['author'] == 'ForkLaunch Team' and manifest['license'] == 'AGPL-3.0'
    assert cargo(app / 'Cargo.toml')['workspace']['package']['license'] == 'AGPL-3.0'

    for kind, directory in [('library', library), ('worker', worker), ('service', service)]:
        before = {p: p.read_bytes() for p in directory.rglob('*.rs')}
        command('change', kind, '-D', f'Updated {kind}', '--confirm', path=directory)
        assert all(p.read_bytes() == source for p, source in before.items()), f'{kind} metadata change replaced application source'
        assert cargo(directory / 'Cargo.toml')['package']['description'] == f'Updated {kind}'
    rejected('change', 'worker', '-d', 'sqlite', path=worker, message='Unsupported Rust worker backend')
    rejected('change', 'worker', '-t', 'redis', path=worker, message='explicit queue/lifecycle migration')
    rejected('change', 'service', '-d', 'sqlite', path=service, message='PostgreSQL')
    # A service with routers converts only after they are removed (ef39d81dd).
    rejected('change', 'service', '--to', 'worker', '-t', 'redis', path=service, message='Delete or move the routers first')

    command('change', 'service', '-N', 'customer-accounts', '--confirm', path=service)
    service = app / 'src/customer-accounts'
    command('change', 'router', '-e', 'users', '-N', 'customers', '--confirm', path=service)
    command('change', 'worker', '-N', 'email-jobs', '--confirm', path=worker)
    worker = app / 'src/email-jobs'
    command('change', 'library', '-N', 'common-core', '--confirm', path=library)
    library = app / 'src/core/common-core'
    for old in ['accounts', 'jobs', 'core/shared']:
        assert not (app / 'src' / old).exists()
    assert 'common_core::custom_value()' in (service / 'lib.rs').read_text()
    assert cargo(service / 'Cargo.toml')['dependencies']['common-core']['path'] == '../core/common-core'
    assert cargo(worker / 'Cargo.toml')['dependencies']['common-core']['path'] == '../core/common-core'
    assert cargo(service / '.forklaunch/contracts/Cargo.toml')['package']['name'] == 'customer-accounts-contracts'
    assert cargo(service / 'sdk/rust/Cargo.toml')['package']['name'] == 'customer-accounts-sdk'
    assert cargo(worker / 'sdk/rust/Cargo.toml')['package']['name'] == 'email-jobs-sdk'
    compose = json.loads(run(['docker', 'compose', 'config', '--no-interpolate', '--format', 'json']).stdout)
    for process, binary in [('customer-accounts', 'customer-accounts'), ('email-jobs', 'email-jobs-worker'), ('email-jobs-server', 'email-jobs')]:
        assert compose['services'][process]['image'] == 'changed-app-rust:local'
        assert compose['services'][process]['command'] == ['/app/bin/' + binary]
    assert set(rust_image()['processes']) == {'customer-accounts', 'email-jobs', 'email-jobs-server'}
    before = tree()
    command('change', 'library', '-N', 'dry-core', '--dryrun', path=library)
    assert tree() == before, 'Dry run mutated the application'
    run(['cargo', 'check', '--workspace', '--all-targets'])
    run(['cargo', 'test', '-p', 'common-core'])
    run(['cargo', 'test', '-p', 'customer-accounts', '--test', 'http'])
    run(['cargo', 'build', '-p', 'customer-accounts', '--bin', 'customer-accounts'])
    run(['node', str(repo / 'cli/tests/fixtures/rust_mcp_service_startup.mjs'), str(Path(env['CARGO_TARGET_DIR']) / 'debug/customer-accounts')])
    command('sdk', 'generate', '--target', 'all', '--from', 'contract')
    manifest = cargo(app / '.forklaunch/manifest.toml')
    assert {p['name'] for p in manifest['projects']} >= {'customer-accounts', 'email-jobs', 'common-core'}
    print('PASS raw mixed application changes, renamed routers/services/workers/libraries, private contract hosts and SDK consumers, source preservation, atomic refusals, dry run, compilation and HTTP/MCP startup', flush=True)
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print('Fixture retained:', root, flush=True)
    else:
        shutil.rmtree(root)

"""Real raw scaffolding and published-surface consumers for builtin runtime gates.

These helpers do not adapt SDK APIs or rewrite contracts. Each runtime gate
imports the emitted types and calls the ordinary public transport directly.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import tomllib
import uuid
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from universal_sdk_source import copy_universal_sdk  # noqa: E402


def raw_application(run, cli, application, name, *, database='postgresql', runtime='node', dryrun=False):
    arguments = [cli, 'init', 'application', name, '--path', application,
                 '--database', database, '--validator', 'zod', '--formatter', 'prettier',
                 '--linter', 'oxlint', '--http-framework', 'express', '--runtime', runtime,
                 '--test-framework', 'vitest', '-D', name, '-A', 'ForkLaunch', '-L', 'MIT']
    if dryrun:
        arguments.append('--dryrun')
    run(*arguments)


def build_public_runtime(run, application, destination, repo):
    """Build the exact bundled public package; return its actual ESM entrypoint."""
    application, destination, repo = map(Path, (application, destination, repo))
    copy_universal_sdk(repo, destination)
    run('npm', 'install', '--prefix', destination, '--ignore-scripts', '--package-lock=false')
    run('npm', '--prefix', destination, 'run', 'build')
    run('node', repo / 'framework/universal-sdk/__test__/publicTransport.mjs', destination)
    # Builtin aggregate SDKs declare their own validator dependencies. Install
    # that emitted package so strict consumers resolve its real runtime imports.
    aggregate = application / 'src/client-sdk/typescript'
    if (aggregate / 'package.json').is_file():
        run('npm', 'install', '--prefix', aggregate, '--ignore-scripts', '--package-lock=false')
    dependency = application / 'node_modules/@forklaunch/universal-sdk'
    dependency.parent.mkdir(parents=True, exist_ok=True)
    assert not dependency.exists(), f'Fixture must not replace an existing dependency: {dependency}'
    dependency.symlink_to(destination, target_is_directory=True)
    return destination / 'lib/index.mjs'


def typecheck(run, application, runtime, files):
    """Check real emitted SDK type exports and gate-specific positive/negative use."""
    runtime, application = Path(runtime), Path(application)
    config = application / 'runtime-typecheck.json'
    config.write_text(json.dumps({'compilerOptions': {
        'strict': True, 'skipLibCheck': True, 'noEmit': True, 'target': 'ES2022',
        'module': 'ESNext', 'moduleResolution': 'Bundler', 'lib': ['ES2022', 'DOM'],
        'types': ['node'], 'typeRoots': [str(runtime / 'node_modules/@types')],
    }, 'files': [str(Path(file).resolve()) for file in files]}, indent=2) + '\n')
    run(os.environ.get('TSC_BIN', runtime / 'node_modules/.bin/tsc'), '--project', config)


def sdk_type_inputs(application, services):
    """Use each service's actual type exports, or the supported legacy aggregate."""
    application = Path(application)
    if all((application / 'src' / name / 'sdk/rust/.forklaunch-sdk-local').is_file()
           for name in services):
        return [application / 'src' / name / 'sdk/typescript/index.ts' for name in services]
    return [application / 'src/client-sdk/typescript/index.ts']


def contract_artifact(application, service, filename='contract.json'):
    application = Path(application)
    project = application / 'src' / service
    if (project / 'sdk/rust/.forklaunch-sdk-local').is_file():
        return project / 'sdk/typescript' / filename
    return application / '.forklaunch/contracts' / service / filename


def rust_consumer(application, destination, service, *, package=None, test_source=None):
    """Ordinary external crate consuming the actual generated Rust SDK.

    The returned manifest can be passed directly to cargo check/test; no generated
    package, application workspace entry, copied SDK, or compatibility API is added.
    """
    application, destination = Path(application), Path(destination)
    local_sdk = application / 'src' / service / 'sdk/rust'
    if (local_sdk / '.forklaunch-sdk-local').is_file():
        assert (local_sdk / 'Cargo.toml').is_file()
        destination.mkdir(parents=True, exist_ok=True)
        name = package or service + '-sdk-consumer'
        if not name.endswith('-consumer'):
            name += '-consumer'
        (destination / 'Cargo.toml').write_text(f'''[package]
name = {json.dumps(name)}
version = "0.0.0"
edition = "2024"
[workspace]
[lib]
path = "lib.rs"
[dependencies]
{json.dumps(service + '-sdk')} = {{path={json.dumps(str(local_sdk))}}}
serde_json = "1"
tokio = {{version="1",features=["macros","rt-multi-thread"]}}
''')
        (destination / 'lib.rs').write_text('//! External consumer of the service-owned SDK crate.\n')
        if test_source is not None:
            (destination / 'tests').mkdir(exist_ok=True)
            (destination / 'tests/runtime.rs').write_text(test_source)
        return destination / 'Cargo.toml'
    # Retain the supported aggregate layout for compatibility fixture captures.
    generated = application / 'src/client-sdk/rust' / f'{service}.rs'
    assert generated.is_file(), f'Compilation did not emit {generated}'
    # Applications no longer vendor framework crates; use the checkout's sources.
    framework = Path(__file__).resolve().parents[3] / 'framework-rs/crates'
    assert (framework / 'core/Cargo.toml').is_file()
    destination.mkdir(parents=True, exist_ok=True)
    dependencies = f'''[package]
name = {json.dumps(package or service + '-sdk-consumer')}
version = "0.0.0"
edition = "2024"
[workspace]
[lib]
path = "lib.rs"
[dependencies]
forklaunch-core = {{path={json.dumps(str(framework / 'core'))},features=["schema-validation"]}}
forklaunch-universal-sdk = {{path={json.dumps(str(framework / 'universal-sdk'))}}}
reqwest = {{version="0.13.2",features=["json","query","multipart"]}}
serde = {{version="1",features=["derive"]}}
serde_json = "1"
tokio = {{version="1",features=["macros","rt-multi-thread"]}}
multer = "3.1"
futures-util = "0.3"
'''
    (destination / 'Cargo.toml').write_text(dependencies)
    (destination / 'lib.rs').write_text(f'#[path={json.dumps(str(generated))}]\nmod sdk;\npub use sdk::*;\n')
    if test_source is not None:
        (destination / 'tests').mkdir(exist_ok=True)
        (destination / 'tests/runtime.rs').write_text(test_source)
    return destination / 'Cargo.toml'


class PostgresDatabase:
    """Isolated real PostgreSQL, or the explicit CI-provided database URL."""
    def __init__(self, environment, prefix):
        self.environment = environment
        self.name = None
        self.prefix = prefix

    def start(self):
        if self.environment.get('FORKLAUNCH_TEST_POSTGRES_URL'):
            return
        self.name = self.prefix + '-' + uuid.uuid4().hex[:12]
        subprocess.run(['docker', 'run', '--detach', '--name', self.name, '--publish', '127.0.0.1::5432',
                        '--tmpfs', '/var/lib/postgresql/data:rw,size=256m', '--env', 'POSTGRES_PASSWORD=parity-test-only',
                        '--env', 'POSTGRES_DB=parity', 'postgres:16-alpine'], check=True, timeout=300)
        for _ in range(60):
            ready = subprocess.run(['docker', 'exec', self.name, 'pg_isready', '-U', 'postgres', '-d', 'parity'],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
            if ready.returncode == 0:
                break
            time.sleep(1)
        else:
            raise AssertionError('Owned PostgreSQL did not become ready')
        address = subprocess.check_output(['docker', 'port', self.name, '5432/tcp'], text=True, timeout=15).strip()
        self.environment['FORKLAUNCH_TEST_POSTGRES_URL'] = f'postgres://postgres:parity-test-only@{address}/parity'

    def close(self):
        if self.name is not None:
            subprocess.run(['docker', 'rm', '-f', self.name], check=False, timeout=60,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

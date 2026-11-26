#!/usr/bin/env python3
"""Raw commands: local SDK Cargo graph, clean build, artifact/check ownership."""
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import tomllib

repo = pathlib.Path(__file__).resolve().parents[3]
cli = pathlib.Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch'))
base = pathlib.Path(tempfile.mkdtemp(prefix='forklaunch-local-sdk-'))
app = base / 'app'
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'CARGO_INCREMENTAL': '0', 'CARGO_TARGET_DIR': str(repo / 'cli/target')}

def run(*args, cwd=None):
    subprocess.run(args, cwd=cwd or repo, env=env, check=True)

def command(*args):
    run(str(cli), *args, '--path', str(app))

try:
    command('init', 'application', 'sdk-proof', '-d', 'postgresql', '-v', 'zod', '-f', 'prettier', '-l', 'oxlint', '-F', 'express', '-r', 'node', '-t', 'vitest', '-D', 'Local SDK proof', '-A', 'ForkLaunch', '-L', 'MIT')
    command('init', 'service', 'api', '--language', 'rust', '-D', 'API')
    command('init', 'library', 'client', '--language', 'rust', '--sdk-services', 'api', '-D', 'Client exports')
    local = app / 'src/api/sdk/rust'
    aggregate = app / 'src/client-sdk/rust'
    local_manifest = tomllib.loads((local / 'Cargo.toml').read_text())
    aggregate_manifest = tomllib.loads((aggregate / 'Cargo.toml').read_text())
    assert local_manifest['build-dependencies']['contracts']['path'] == '../../.forklaunch/contracts'
    assert 'api' not in local_manifest['dependencies']
    assert 'build-dependencies' not in aggregate_manifest
    assert aggregate_manifest['dependencies']['api']['path'] == '../../api/sdk/rust'
    assert not (aggregate / 'build.rs').exists()
    assert not (app / 'src/api-contracts').exists()
    assert not (local / 'generated.rs').exists(), 'First SDK build must not require persistent generated sources'
    run('cargo', '+stable', 'check', '-p', 'api-sdk', '-p', 'client', cwd=app)
    assert (app / 'src/api/sdk/typescript/sdk.ts').is_file(), 'SDK-only build must emit native type declarations'
    run('cargo', '+stable', 'check', '-p', 'api', cwd=app)
    assert (local / 'generated.rs').is_file()
    ts = app / 'src/api/sdk/typescript'
    assert json.loads((ts / 'package.json').read_text())['name'] == '@sdk-proof/api'
    assert json.loads((ts / 'contract.json').read_text())['sdkLayout'] == 'project-local-v1'
    assert (ts / 'sdk.ts').is_file()
    assert not (ts / 'client.ts').exists() and not (ts / 'runtime.ts').exists()
    assert not (app / 'src/client-sdk/rust/api.rs').exists()
    command('sdk', 'generate', '--target', 'all', '--from', 'contract', '--service', 'api', '--check')
    command('init', 'service', 'portal', '--language', 'typescript', '-d', 'postgresql', '-D', 'TypeScript consumer')
    docker = (app / 'Dockerfile.typescript').read_text()
    assert 'RUN cargo check --locked -p api-sdk' in docker
    assert 'COPY --from=native-sdk-compiler [\"/app/src/api/sdk/typescript/\",\"/app/src/api/sdk/typescript/\"]' in docker
    assert docker.index('RUN cargo check --locked -p api-sdk') < docker.index('FROM node:')
    print('PASS: clean local SDK + dependency-only aggregate compile, service-owned artifacts, CLI freshness')
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print('Fixture retained:', app)
    else:
        shutil.rmtree(base)

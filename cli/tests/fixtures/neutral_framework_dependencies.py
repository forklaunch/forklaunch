#!/usr/bin/env python3
"""Dependency preferences on neutral init activate only when a Rust project is added."""
import json, os, pathlib, shutil, subprocess, tempfile, tomllib
repo = pathlib.Path(__file__).resolve().parents[3]
cli = pathlib.Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch'))
root = pathlib.Path(tempfile.mkdtemp(prefix='forklaunch-neutral-dependencies-'))
# The CLI's defaults track the workspace versions (enforced by CLI unit tests).
framework_version = tomllib.loads((repo / 'framework-rs/Cargo.toml').read_text())['workspace']['package']['version']
blueprint_version = tomllib.loads((repo / 'blueprint-rs/Cargo.toml').read_text())['workspace']['package']['version']
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'CARGO_INCREMENTAL': '0', 'CARGO_TARGET_DIR': str(repo / 'cli/target')}
def run(*args):
    result = subprocess.run(list(map(str, args)), env=env, text=True, capture_output=True)
    assert result.returncode == 0, result.stdout[-3000:] + result.stderr[-12000:]
    return result.stdout
try:
    for profile in ('local', 'registry'):
        app = root / profile
        option = ('--framework-path', repo / 'framework-rs') if profile == 'local' else ('--framework-version', framework_version)
        run(cli, 'init', 'application', 'neutral-proof', *option, '--path', app,
            '--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint',
            '--http-framework', 'express', '--runtime', 'node', '--test-framework', 'vitest', '-D', 'Dependency proof', '-A', 'ForkLaunch', '-L', 'MIT')
        assert not (app / '.forklaunch/framework-dependencies.toml').exists()
        config = tomllib.loads((app / '.forklaunch/manifest.toml').read_text())['framework']
        assert config == ({'path': str((repo / 'framework-rs').resolve()), 'blueprint_version': blueprint_version} if profile == 'local' else {'version': framework_version, 'blueprint_version': blueprint_version}), config
        assert not (app / 'Cargo.toml').exists()
        assert not (app / 'vendor').exists()
        manifest = tomllib.loads((app / '.forklaunch/manifest.toml').read_text())
        assert manifest.get('language') not in ('rust', 'mixed')
        run(cli, 'init', 'service', 'api', '--language', 'rust', '--path', app, '-D', 'API')
        assert not (app / 'vendor/framework').exists()
        assert not (app / 'vendor/serde_json').exists()
        if profile == 'local':
            # A local checkout reaches Docker builds as a named build context.
            compose = json.loads(run('docker', 'compose', '-f', app / 'docker-compose.yaml', 'config', '--format', 'json', '--no-interpolate', '--no-env-resolution'))
            build = compose['services']['api']['build']
            assert build.get('additional_contexts', {}).get('framework') == str((repo / 'framework-rs').resolve()), build
        cargo = tomllib.loads((app / 'src/api/Cargo.toml').read_text())
        core = next(dep for name, dep in cargo['dependencies'].items() if isinstance(dep, dict) and (name == 'forklaunch-core' or dep.get('package') == 'forklaunch-core'))
        if profile == 'local': assert core['path'] == str((repo / 'framework-rs/crates/core').resolve()), core
        else: assert core['version'] == f'={framework_version}' and 'path' not in core, core
        metadata = json.loads(run('cargo', '+stable', 'metadata', '--no-deps', '--format-version', '1', '--manifest-path', app / 'Cargo.toml'))
        assert {'api', 'api-contracts'} <= {package['name'] for package in metadata['packages']}
        if profile == 'local':
            run('cargo', '+stable', 'check', '--manifest-path', app / 'Cargo.toml', '-p', 'api-contracts')
        print('PASS neutral application persists dependency choice; later Rust project consumes it:', profile, flush=True)
    print('Registry generation/metadata only: no publication or registry installation claimed.', flush=True)
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'): print('Fixture retained:', root, flush=True)
    else: shutil.rmtree(root)

#!/usr/bin/env python3
"""Only ordinary init commands: project language order cannot split source roots/images."""
import json, os, pathlib, shutil, subprocess, tempfile, tomllib
repo = pathlib.Path(__file__).resolve().parents[3]
cli = pathlib.Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch'))
root = pathlib.Path(tempfile.mkdtemp(prefix='forklaunch-raw-mixed-'))
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'CARGO_INCREMENTAL': '0', 'CARGO_TARGET_DIR': str(repo / 'cli/target')}
def command(app, *args):
    result = subprocess.run([str(cli), *args, '--path', str(app)], env=env, text=True, capture_output=True)
    assert result.returncode == 0, result.stdout[-4000:] + result.stderr[-10000:]
    return result.stdout
try:
    for case in ('rust-first', 'typescript-first', 'legacy-rust-first'):
        app = root / case
        options = ['--language', 'rust'] if case == 'legacy-rust-first' else ['--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint', '--http-framework', 'express', '--runtime', 'node', '--test-framework', 'vitest']
        command(app, 'init', 'application', 'workspace-proof', *options, '-D', 'Raw workspace proof', '-A', 'ForkLaunch', '-L', 'MIT')
        manifest = tomllib.loads((app / '.forklaunch/manifest.toml').read_text())
        assert manifest['modules_path'] == 'src', manifest
        assert set(manifest.get('sdkTargets', manifest.get('sdk_targets', []))) == {'typescript', 'rust'}, manifest
        assert (app / 'Dockerfile').is_file() and not (app / 'src/Dockerfile').exists()
        steps = [('service', 'api', '--language', 'rust', '-D', 'API'), ('module', '--language', 'typescript', '-m', 'iam-better-auth', '-d', 'postgresql')]
        if case == 'typescript-first': steps.reverse()
        for step in steps: command(app, 'init', *step)
        manifest = tomllib.loads((app / '.forklaunch/manifest.toml').read_text())
        assert manifest['modules_path'] == 'src'
        assert manifest['language'] == 'mixed', manifest
        scripts = json.loads((app / 'package.json').read_text())['scripts']
        assert '--target all' in scripts['sdk:generate'], scripts
        assert '--target all' in scripts['sdk:check'], scripts
        cargo = tomllib.loads((app / 'Cargo.toml').read_text())
        assert 'vendor/framework' not in cargo['workspace'].get('exclude', []), cargo
        assert (app / 'src/api/Cargo.toml').is_file()
        # Contract hosts live inside the service, not as a sibling crate.
        assert (app / 'src/api/.forklaunch/contracts/Cargo.toml').is_file()
        assert not (app / 'src/api-contracts').exists()
        assert not (app / 'src/api/domain/Cargo.toml').exists()
        assert (app / 'src/iam/package.json').is_file()
        assert not (app / 'modules').exists()
        assert (app / 'src/client-sdk').is_dir()
        nested_workspace = (app / 'src/pnpm-workspace.yaml').read_text()
        for member in ('client-sdk/typescript', 'client-sdk/events/typescript', '*/events/typescript'):
            assert member in nested_workspace, nested_workspace
        root_workspace = (app / 'pnpm-workspace.yaml').read_text()
        assert 'src/client-sdk/typescript' in root_workspace
        assert 'src/*/events/typescript' in root_workspace
        assert (app / 'Dockerfile.rust').is_file() and (app / 'Dockerfile.typescript').is_file()
        assert not (app / 'Dockerfile').exists() and not (app / 'src/Dockerfile').exists()
        compose = json.loads(subprocess.check_output(['docker', 'compose', '-f', str(app / 'docker-compose.yaml'), 'config', '--format', 'json', '--no-interpolate', '--no-env-resolution'], env=env, text=True))
        native = compose['services']['api']
        assert pathlib.Path(native['build']['dockerfile']).name == 'Dockerfile.rust', native
        assert native['command'][0] == '/app/bin/api', native
        typescript = [service for service in compose['services'].values() if service.get('command', [''])[0] in ('node', 'bun')]
        assert typescript, compose['services']
        assert all(service.get('image') == 'workspace-proof-typescript:local' for service in typescript)
        assert all(pathlib.Path(service['build']['dockerfile']).name == 'Dockerfile.typescript' for service in typescript if 'build' in service)
        assert 'src/api' in (app / 'Dockerfile.rust').read_text()
        assert not (app / 'generation.json').exists()
        print('PASS raw project language order, shared src, dual SDK defaults and language images:', case, flush=True)
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'): print('Fixture retained:', root, flush=True)
    else: shutil.rmtree(root)

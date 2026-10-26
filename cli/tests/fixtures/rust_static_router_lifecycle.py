#!/usr/bin/env python3
"""Actual ordinary router lifecycle compiles complete static binding manifests."""
import json, os, pathlib, shlex, shutil, subprocess, sys, tempfile
repo = pathlib.Path(__file__).resolve().parents[3]
cli = pathlib.Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch'))
root = pathlib.Path(tempfile.mkdtemp(prefix='forklaunch-static-router-'))
app = root / 'app'
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'CARGO_INCREMENTAL': '0', 'CARGO_TARGET_DIR': str(repo / 'cli/target')}
def run(args):
    result = subprocess.run(args, cwd=repo, env=env, capture_output=True, text=True)
    assert result.returncode == 0, result.stdout[-4000:] + result.stderr[-9000:]
def command(*args, path=app):
    run([str(cli), *args, '--path', str(path)])
def check(names, package='api'):
    run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'), '-c', shlex.join(['cargo', '+stable', 'check', '--manifest-path', str(app / 'Cargo.toml'), '-p', package])])
    manifest = json.loads((app / f'src/{package}/sdk/typescript/bindings.json').read_text())
    assert sorted(item['member'] for item in manifest['declarations']) == sorted(names), manifest
try:
    command('init', 'application', 'router-proof', '--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint', '--http-framework', 'express', '--runtime', 'node', '--test-framework', 'vitest', '-D', 'Router proof', '-A', 'ForkLaunch', '-L', 'MIT')
    command('init', 'service', 'api', '--language', 'rust', '-D', 'API')
    service = app / 'src/api'
    assert (app / 'src/api/.forklaunch/contracts/lib.rs').is_file()
    assert not (app / 'src/api-contracts').exists()
    assert (service / 'domain/schemas/mod.rs').is_file()
    assert 'pub mod contract' in (service / 'api/controllers/health.rs').read_text()
    assert not (service / 'api/contracts').exists()
    assert not (service / 'domain/Cargo.toml').exists()
    run(['cargo', '+stable', 'fmt', '--manifest-path', str(app / 'Cargo.toml'), '--all'])
    assert not (service / 'domain/types').exists()
    command('init', 'router', 'comments', '--language', 'rust', path=service)
    check(['health', 'list'])
    command('sdk', 'generate', '--check', '--target', 'all', '--from', 'all', '--service', 'api')
    controller = service / 'api/controllers/comments.rs'
    controller.write_text(controller.read_text() + '\n// application-owned controller\n')
    command('change', 'router', '-e', 'comments', '-N', 'replies', '--confirm', path=service)
    assert 'application-owned controller' in (service / 'api/controllers/replies.rs').read_text()
    check(['health', 'list'])
    manifest = json.loads((app / 'src/api/sdk/typescript/bindings.json').read_text())
    assert any('replies.rs' in item['source'] for item in manifest['declarations'])
    assert all('comments.rs' not in item['source'] for item in manifest['declarations'])
    command('delete', 'router', 'replies', '--continue', path=service)
    check(['health'])
    command('init', 'library', 'notes-clients', '--language', 'rust', '--sdk-services', 'api')
    run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'), '-c', shlex.join(['cargo', '+stable', 'check', '--manifest-path', str(app / 'Cargo.toml'), '--workspace'])])
    command('change', 'service', '-N', 'records', '--confirm', path=service)
    assert (app / 'src/records/.forklaunch/contracts/lib.rs').is_file()
    assert not (app / 'src/api/.forklaunch/contracts').exists()
    check(['health'], 'records')
    command('sdk', 'generate', '--check', '--target', 'all', '--from', 'all', '--service', 'records')
    run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'), '-c', shlex.join(['cargo', '+stable', 'check', '--manifest-path', str(app / 'Cargo.toml'), '--workspace'])])
    command('delete', 'library', 'notes-clients', '--continue')
    command('delete', 'service', 'records', '--continue')
    assert not (app / 'src/records').exists()
    assert not (app / 'src/records/.forklaunch/contracts').exists()
    print('PASS ordinary router and sibling service init/rename/delete compile exact bindings and preserve custom controller', flush=True)
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print('Fixture retained:', root, flush=True)
    else:
        shutil.rmtree(root)

#!/usr/bin/env python3
"""Raw native generation preserves composition, startup, export and secured listeners."""
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

repo = Path(__file__).resolve().parents[3]
cli = Path(os.environ['FORKLAUNCH_CLI'])
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'CARGO_INCREMENTAL': '0', 'RUSTUP_TOOLCHAIN': 'stable'}
env.setdefault('CARGO_TARGET_DIR', str(repo / 'cli/target'))
if not os.environ.get('FORKLAUNCH_WORKER_RUNTIME_LOCKED'):
    # Hold the lock through both compilation and execution of shared binaries.
    result = subprocess.run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'), '-c', shlex.join([sys.executable, str(Path(__file__).resolve())])], env=env)
    sys.exit(result.returncode)
root = Path(tempfile.mkdtemp(prefix='forklaunch-bootstrap-'))
app = root / 'app'

def run(args, **kwargs):
    print('Running:', ' '.join(map(str, args)), flush=True)
    return subprocess.run(args, cwd=app if app.exists() else repo, env=env, check=True, **kwargs)

def command(*args):
    run([str(cli), *args, '--path', str(app)])

try:
    command('init', 'application', 'notes-app', '--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint', '--http-framework', 'express', '--runtime', 'node', '--test-framework', 'vitest', '-D', 'Bootstrap proof', '-A', 'ForkLaunch', '-L', 'MIT')
    command('init', 'service', 'probe', '--language', 'rust', '-D', 'Startup probe')
    command('init', 'service', 'notes', '--language', 'rust', '--database', 'postgresql', '--mappers', '-D', 'Notes service')
    command('init', 'worker', 'insights', '--language', 'rust', '--type', 'database', '--database', 'postgresql', '--runtime', 'apalis', '-D', 'Note insights producer and consumer')
    for name in ['probe', 'notes', 'insights']:
        source = app / 'src' / name
        application = (source / 'application.rs').read_text()
        bootstrap = (source / 'bootstrapper.rs').read_text()
        assert 'pub async fn run' not in application
        assert 'std::env::var' not in application and 'TcpListener' not in application
        assert 'config.origin()' in application
        assert 'ServiceConfig::from_env()' in bootstrap and 'TcpListener::bind' in bootstrap
        assert 'start()' in bootstrap and 'application::run' not in bootstrap
        assert (source / 'error.rs').exists()
    stored = (app / 'src/notes/bootstrapper.rs').read_text()
    assert 'with_cleanup(' in stored and 'repository.close()' in stored
    assert 'migrate().await?' in stored and 'register(config).await?' in stored
    run(['cargo', 'fmt', '--all'])
    run(['cargo', 'check', '--workspace', '--all-targets'])
    run(['cargo', 'test', '-p', 'probe', '--test', 'http'])
    run(['cargo', 'test', '-p', 'notes', '--test', 'http'])
    run(['cargo', 'build', '-p', 'probe', '-p', 'notes', '-p', 'insights', '--bins'])
    binaries = Path(env['CARGO_TARGET_DIR']) / 'debug'
    run(['node', str(repo / 'cli/tests/fixtures/rust_mcp_service_startup.mjs'), str(binaries / 'probe')])
    run(['node', str(repo / 'cli/tests/fixtures/rust_worker_service_policy.mjs'), str(binaries / 'insights')])
    clean_env = {key: value for key, value in env.items() if not key.startswith(('OTEL_', 'MCP_', 'FORKLAUNCH_MODE', 'FORKLAUNCH_CONTRACT_OUTPUT', 'FORKLAUNCH_OPENAPI_OUTPUT')) and key not in {'DATABASE_URL', 'MIGRATION_DATABASE_URL', 'TENANT_ID'}}
    for binary in ['probe', 'notes', 'insights']:
        contract = root / f'{binary}-contract.json'
        openapi = root / f'{binary}-openapi.json'
        exported = subprocess.run([str(binaries / binary), '--export-contracts'], cwd=app, env={**clean_env, 'OTEL_EXPORTER_OTLP_PROTOCOL':'invalid', 'SERVICE_HOST':'', 'FORKLAUNCH_CONTRACT_OUTPUT':str(contract), 'FORKLAUNCH_OPENAPI_OUTPUT':str(openapi)}, capture_output=True, text=True, timeout=20)
        assert exported.returncode == 0, exported.stderr
        assert json.loads(contract.read_text()) and json.loads(openapi.read_text())
        assert 'listening on' not in exported.stdout
    for arguments, expected in [(['--unknown'], 'Expected no arguments or --migrate'), (['--migrate'], 'environment variable MIGRATION_DATABASE_URL is not set')]:
        # RUST_LOG is a required startup setting (7248a878d); provide it so these
        # probes reach argument and migration-URL validation.
        failed = subprocess.run([str(binaries / 'notes'), *arguments], cwd=app, env={**clean_env, 'RUST_LOG': 'info'}, capture_output=True, text=True, timeout=20)
        assert failed.returncode != 0 and expected in failed.stderr, failed.stderr
        assert 'listening on' not in failed.stdout
    # Identical generated package names can share a build cache across workspaces.
    # Keep both source trees present so removing the old source cannot hide stale
    # paths in a reused contract host or build-script executable.
    relocated = root / 'relocated'
    shutil.copytree(app, relocated, ignore=shutil.ignore_patterns('target', 'node_modules'))
    for output in ['.forklaunch/compliance.json', 'sdk/typescript/contract.json']:
        (relocated / 'src/insights' / output).unlink()
    for workspace in [app, relocated, app]:
        print('Checking relocated contract host:', workspace, flush=True)
        subprocess.run(['cargo', 'check', '-p', 'insights', '-p', 'insights-sdk', '--all-targets'], cwd=workspace, env=env, check=True)
        evidence = json.loads((workspace / 'src/insights/sdk/typescript/provenance.json').read_text())
        assert evidence['outputRoot'] == 'application', 'Build assets must belong to the current workspace, even with a cached build-script executable'
        assert (workspace / 'src/insights/.forklaunch/compliance.json').is_file()
        assert (workspace / 'src/insights/sdk/typescript/contract.json').is_file()
    print('PASS: raw service/worker composition, real HTTP/MCP listeners, graceful shutdown, producer denial, export before startup, and migration dispatch', flush=True)
    print('PASS: same-named contract hosts and build outputs remain valid across relocated workspaces sharing a Cargo target', flush=True)
    if os.environ.get('FORKLAUNCH_BOOTSTRAP_FIXTURE_OUTPUT'):
        Path(os.environ['FORKLAUNCH_BOOTSTRAP_FIXTURE_OUTPUT']).write_text(str(root)+'\n')
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print('Fixture retained:', root, flush=True)
    else:
        shutil.rmtree(root)

#!/usr/bin/env python3
"""Ordinary --mappers projection is checked and used; disabled services stay direct."""
import os, pathlib, shlex, shutil, subprocess, sys, tempfile
repo = pathlib.Path(__file__).resolve().parents[3]
cli = pathlib.Path(os.environ['FORKLAUNCH_CLI'])
root = pathlib.Path(tempfile.mkdtemp(prefix='forklaunch-checked-mapper-'))
app = root / 'app'
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'CARGO_INCREMENTAL': '0'}
env.setdefault('CARGO_TARGET_DIR', str(repo / 'cli/target'))
def run(args):
    print('+', shlex.join(args), flush=True)
    result = subprocess.run(args, cwd=repo, env=env, capture_output=True, text=True)
    assert result.returncode == 0, result.stdout[-3000:] + result.stderr[-9000:]
def command(*args):
    run([str(cli), *args, '--path', str(app)])
def cargo(*args):
    run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'), '-c', shlex.join(['cargo', '+stable', *args, '--manifest-path', str(app / 'Cargo.toml')])])
try:
    command('init', 'application', 'mapper-proof', '--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint', '--http-framework', 'express', '--runtime', 'node', '--test-framework', 'vitest', '-D', 'Mapper proof', '-A', 'ForkLaunch', '-L', 'MIT')
    command('init', 'service', 'mapped', '--language', 'rust', '--mappers', '-D', 'Mapped')
    command('init', 'service', 'direct', '--language', 'rust', '-D', 'Direct')
    command('init', 'service', 'stored', '--language', 'rust', '--database', 'postgresql', '--mappers', '-D', 'Stored')
    assert (app/'src/mapped/domain/mappers/health.rs').is_file()
    assert not (app/'src/direct/domain/mappers').exists()
    assert 'mod mappers' not in (app/'src/direct/domain/mod.rs').read_text()
    cargo('test', '-p', 'mapped', '--lib')
    cargo('test', '-p', 'mapped', '--test', 'http')
    cargo('test', '-p', 'stored', '--lib')
    cargo('test', '-p', 'direct', '--test', 'http')
    print('PASS checked health projection validates values for memory and PostgreSQL09; mapped and unmapped HTTP surfaces work')
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print('Fixture retained:', root)
    else:
        shutil.rmtree(root)

#!/usr/bin/env python3
"""Generate every Rust project kind into one application and hold the output to
the bar its generated CI sets: formatting, Clippy, tests and HTTP/MCP startup.

Framework and blueprint come from this checkout (`--framework-path`), so the
check covers unpublished changes. Set FORKLAUNCH_CLI to the CLI under test."""
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

repo = Path(__file__).resolve().parents[3]
cli = Path(os.environ['FORKLAUNCH_CLI']).resolve()
env = {**os.environ, 'RUSTUP_TOOLCHAIN': os.environ.get('RUSTUP_TOOLCHAIN', 'stable'), 'CARGO_INCREMENTAL': '0'}
for variable in ('FORKLAUNCH_SKIP_FORMAT', 'FORKLAUNCH_CONTRACT_COMPILER'):
    env.pop(variable, None)
root = Path(tempfile.mkdtemp(prefix='forklaunch-generated-projects-'))
app = root / 'app'
env.setdefault('CARGO_TARGET_DIR', str(root / 'target'))


def run(*command, cwd=None, **kwargs):
    print('Running:', shlex.join(map(str, command)), flush=True)
    return subprocess.run(list(map(str, command)), cwd=cwd or (app if app.exists() else root), env=env, check=True, **kwargs)


def forklaunch(*arguments):
    run(cli, *arguments, '--path', app)


# Every generator, including the worker type that maps onto another backend.
PROJECTS = [
    ('service', 'notes', ['--database', 'postgresql', '--infrastructure', 'redis', 's3', '--mappers']),
    ('service', 'plain', []),
    # An agent over the plain service and one external MCP server.
    ('agent', 'assistant', ['--service', 'plain', '--mcp', 'linear=https://mcp.linear.app/mcp']),
    ('worker', 'jobs', ['--type', 'database', '--database', 'postgresql', '--runtime', 'apalis']),
    ('worker', 'cache-jobs', ['--type', 'redis']),
    ('worker', 'stream-jobs', ['--type', 'kafka']),
    ('worker', 'queue-jobs', ['--type', 'bullmq']),
]
MODULES = [('iam', 'iam-base'), ('cac', 'cac-base'), ('shop', 'ecommerce-stripe'),
           ('billing', 'billing-stripe'), ('ledger', 'billing-base')]

try:
    run(cli, 'init', 'application', 'generated-projects', '--language', 'rust',
        '--framework-path', repo / 'framework-rs', '-D', 'Generated projects', '-A', 'ForkLaunch', '-L', 'MIT', '--path', app)
    for kind, name, options in PROJECTS:
        forklaunch('init', kind, name, '--language', 'rust', *options, '-D', f'Generated {name}')
    for name, module in MODULES:
        forklaunch('init', 'module', name, '--language', 'rust', '-m', module, '-d', 'postgresql')
    forklaunch('init', 'library', 'shared', '--language', 'rust', '-D', 'Shared library')
    run(cli, 'infra', 'add', 'billing', 'payments', '--path', app)
    run(cli, 'infra', 'add', 'notes', 'email', '--path', app)

    manifest = (app / '.forklaunch/manifest.toml').read_text()
    assert 'worker-bullmq' not in manifest, 'Rust BullMQ workers use the native Redis queue'
    assert not (app / '.forklaunch/blueprint-rs').exists(), 'Blueprint crates resolve from the blueprint workspace'
    for name, _ in MODULES:
        assert (app / f'src/{name}/registrations.rs').is_file(), f'{name} uses the service layout'
        assert not (app / f'src/{name}/api/http').exists(), f'{name} binds blueprint handlers instead of copying them'
    assert 'variant = "agent"' in manifest, 'Agents are services tagged variant = "agent"'
    for path in ('agent/mod.rs', 'agent/tools/current_time.rs', '__test__/agent.rs'):
        assert (app / 'src/assistant' / path).is_file(), f'agent {path}'
    assert 'agent::contract::routes()' in (app / 'src/assistant/.forklaunch/contracts/lib.rs').read_text()

    # Generated bodies name abstractions they import, not fully qualified paths.
    qualified = re.compile(r'(?<![\w:"])(?:forklaunch_[a-z_]+|std|tokio)::(?:[a-z_]\w*::)+[A-Z]\w*')
    findings = []
    for path in sorted((app / 'src').rglob('*.rs')):
        # Build-owned output (.forklaunch) is not edited by hand.
        if 'target' in path.parts or '.forklaunch' in path.parts:
            continue
        for number, line in enumerate(path.read_text().splitlines(), 1):
            stripped = line.strip()
            if stripped.startswith(('use ', 'pub use ', '//', '#[', '#![')) or 'tokio::signal' in line or 'std::os' in line:
                continue
            if qualified.search(line.split('//')[0]):
                findings.append(f'{path.relative_to(app)}:{number}: {stripped[:120]}')

    assert not findings, 'Generated bodies use fully qualified paths:\n' + '\n'.join(findings)
    run('cargo', 'generate-lockfile')
    run('cargo', 'fmt', '--', '--check')
    clippy = subprocess.run(['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--message-format', 'short'],
                            cwd=app, env=env, capture_output=True, text=True)
    sys.stdout.write(clippy.stderr[-4000:])
    assert clippy.returncode == 0, 'Clippy failed'
    # Short-format paths are workspace-relative, and the contract hosts reach
    # into service source through `.forklaunch/contracts/../..`.
    warnings = sorted({line for line in clippy.stderr.splitlines() if ': warning: ' in line and line.startswith('src/')})
    assert not warnings, 'Clippy warnings in generated source:\n' + '\n'.join(warnings)
    tests = run('cargo', 'test', '--workspace', '--locked', capture_output=True, text=True)
    passed = sum(int(m) for m in re.findall(r'test result: ok\. (\d+) passed', tests.stdout))
    run('cargo', 'build', '-p', 'plain', '--bins')
    run('node', repo / 'cli/tests/fixtures/rust_mcp_service_startup.mjs', Path(env['CARGO_TARGET_DIR']) / 'debug/plain')
    receipt = {
        'projects': [name for _, name, _ in PROJECTS] + [name for name, _ in MODULES] + ['shared'],
        'testsPassed': passed, 'clippyWarningsInGeneratedSource': warnings,
        'qualifiedPathsInBodies': findings,
    }
    print(json.dumps(receipt, indent=2))
    if os.environ.get('FORKLAUNCH_GENERATED_RECEIPT'):
        Path(os.environ['FORKLAUNCH_GENERATED_RECEIPT']).write_text(json.dumps(receipt, indent=2) + '\n')
    print(f'PASS generated projects: fmt, Clippy, {passed} tests and HTTP/MCP startup; '
          f'{len(warnings)} Clippy warnings and no qualified body paths in generated source', flush=True)
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print('Fixture retained:', root, flush=True)
    else:
        shutil.rmtree(root)

#!/usr/bin/env python3
"""Code-editing commands on a Rust application: change application/service/
worker/router/library and delete, checked for leftovers after every step.

Each command runs with --dryrun first (where supported) and must not write;
Compose must stay valid after every step; the workspace must build and pass
clippy at the end (skip with --skip-cargo).

Every case is recorded with its result, duration and, on failure, the check
that failed (dry run, compose config, leftovers, build, clippy or an
assertion) and the first lines of the error; later cases still run. With
GITHUB_STEP_SUMMARY set, the results become a table in the job summary and
each failure a GitHub error annotation. --group change|delete runs half of
the sweep (delete repeats the changes without building, then deletes).
"""
import argparse
import contextlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import tomllib
import traceback

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--skip-cargo', action='store_true')
parser.add_argument('--keep', action='store_true')
parser.add_argument('--group', choices=['change', 'delete', 'all'], default='all')
args = parser.parse_args()
repo = Path(__file__).resolve().parents[3]
cli = Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch')).resolve()
env = {**os.environ, 'RUSTUP_TOOLCHAIN': 'stable', 'FORKLAUNCH_SKIP_PRECHECK': '1', 'CARGO_INCREMENTAL': '0'}
root = Path(tempfile.mkdtemp(prefix='forklaunch-command-sweep-'))
app = root / 'app'
results = []


def check_of(message):
    text = message.lower()
    for needle, label in [('dry run', 'dry run'), ('left behind', 'leftovers'), ('outlived', 'leftovers'),
                          ('clippy', 'clippy'), ('cargo build', 'build'), ('docker compose', 'compose config')]:
        if needle in text:
            return label
    return 'assertion'


@contextlib.contextmanager
def case(name):
    """Record one command case; a failure is kept and the sweep goes on."""
    started = time.monotonic()
    print(f'--- {name}', flush=True)
    try:
        yield
    except Exception as error:  # noqa: BLE001 - every failure is reported, not raised
        message = str(error) or type(error).__name__
        lines = [line for line in message.splitlines() if line.strip()][:8]
        results.append({'name': name, 'ok': False, 'seconds': time.monotonic() - started,
                        'check': check_of(message), 'error': lines})
        print(f'FAIL {name} ({check_of(message)}):\n' + '\n'.join(lines), flush=True)
        traceback.print_exc()
    else:
        results.append({'name': name, 'ok': True, 'seconds': time.monotonic() - started})
        print(f'PASS {name}', flush=True)


def report():
    failed = [r for r in results if not r['ok']]
    title = f'Rust command sweep ({args.group})'
    rows = [f'### {title}: {len(results) - len(failed)}/{len(results)} cases passed', '',
            '| Case | Result | Time | Failed check |', '| --- | --- | --- | --- |']
    for r in results:
        rows.append(f"| {r['name']} | {'✅' if r['ok'] else '❌'} | {r['seconds']:.0f}s | {'' if r['ok'] else r['check']} |")
    for r in failed:
        rows += ['', f"<details><summary>{r['name']}: {r['check']}</summary>", '', '```',
                 *r['error'], '```', '</details>']
    summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if summary:
        with open(summary, 'a') as out:
            out.write('\n'.join(rows) + '\n')
    for r in failed:
        detail = ' / '.join(r['error'][:3]).replace('%', '%25').replace('\r', '').replace('\n', ' ')
        print(f"::error title={r['name']}::{r['check']} failed: {detail}", flush=True)
    print('\n'.join(rows), flush=True)
    return not failed


def run(arguments, *, cwd=None, check=True):
    result = subprocess.run([str(a) for a in arguments], cwd=cwd or app, env=env,
                            stdin=subprocess.DEVNULL, text=True, capture_output=True)
    if check:
        assert result.returncode == 0, ' '.join(map(str, arguments)) + '\n' + result.stdout[-4000:] + result.stderr[-6000:]
    return result


def tree():
    return {str(p.relative_to(app)): p.stat().st_mtime_ns for p in app.rglob('*')
            if p.is_file() and not {'.git', 'target', 'node_modules'} & set(p.relative_to(app).parts)}


def forklaunch(*arguments, path=None, dryrun=True):
    command = [cli, *arguments, '--path', path or app]
    if dryrun:
        before = tree()
        result = run([*command, '--dryrun'])
        assert tree() == before, f'dry run wrote files: {arguments}'
        assert 'Would ' in result.stdout, f'dry run printed no plan: {arguments}\n{result.stdout}'
    result = run(command)
    run(['docker', 'compose', 'config', '-q'])
    return result


def stale(name, renamed=None):
    """Places a renamed or deleted project must not appear."""
    upper = name.upper().replace('-', '_') + '_'
    problems = []
    cargo = tomllib.loads((app / 'Cargo.toml').read_text())
    problems += [m for m in cargo['workspace']['members'] if re.search(rf'(^|/){re.escape(name)}(/|$)', m)]
    compose = json.loads(run(['docker', 'compose', 'config', '--no-interpolate', '--format', 'json']).stdout)['services']
    for key, service in compose.items():
        if key == name or key.startswith(name + '-'):
            problems.append(f'compose service {key}')
        for dependency in service.get('depends_on', {}):
            if dependency == name or dependency.startswith(name + '-'):
                problems.append(f'{key} depends_on {dependency}')
        for variable, value in (service.get('environment') or {}).items():
            # A module reads fixed setting names (IAM_*) whatever the project is called.
            own = renamed and (key == renamed or key.startswith(renamed + '-'))
            if f'${{{upper}' in str(value) or (variable.startswith(upper) and not own) or f'//{name}:' in str(value):
                problems.append(f'{key} {variable}={value}')
    problems += [line for line in (app / '.env').read_text().splitlines() if line.startswith(upper)]
    manifest = tomllib.loads((app / '.forklaunch/manifest.toml').read_text())
    problems += [f"manifest {p['name']}" for p in manifest['projects'] if p['name'] == name]
    problems += [f'manifest top-level {key}' for key in ['snake_case_name'] if key in manifest]
    for path in ['src/client-sdk/index.ts', 'src/client-sdk/clientSdk.ts', 'src/client-sdk/compliance.ts', 'package.json']:
        text = (app / path).read_text() if (app / path).exists() else ''
        if re.search(rf'[@/"]{re.escape(name)}["/:]', text):
            problems.append(f'{path} mentions {name}')
    assert not problems, f'{name} left behind: {problems}'


try:
    framework = repo / 'framework-rs'
    with case('setup: init application, service, router, workers, module, agent, library'):
        run([cli, 'init', 'application', 'zebra', '--path', app, '-d', 'postgresql', '-v', 'zod', '-f', 'prettier',
             '-l', 'oxlint', '-F', 'express', '-r', 'node', '-t', 'vitest', '-D', 'Sweep', '-A', 'ForkLaunch',
             '-L', 'MIT', '-o', 'src', '--framework-path', framework], cwd=root)
        run([cli, 'init', 'service', 'accounts', '--language', 'rust', '-d', 'postgresql', '-D', 'Accounts', '--path', app])
        run([cli, 'init', 'router', 'ledger', '--language', 'rust', '--path', app / 'src/accounts'])
        run([cli, 'init', 'worker', 'dbjobs', '--language', 'rust', '-t', 'database', '-d', 'postgresql', '-D', 'Jobs', '--path', app])
        run([cli, 'init', 'worker', 'events', '--language', 'rust', '-t', 'kafka', '-D', 'Events', '--path', app])
        run([cli, 'init', 'module', '--language', 'rust', '-m', 'iam-base', '-d', 'postgresql', '--path', app])
        run([cli, 'init', 'agent', 'helper', '--language', 'rust', '-D', 'Helper', '--path', app])
        run([cli, 'init', 'library', 'shared', '--language', 'rust', '-D', 'Shared', '--path', app])
    if not results[-1]['ok']:
        raise SystemExit(0 if report() else 1)

    # sync all keeps the manifest's key and table order (it rewrote the file
    # in its own order).
    def layout():
        text = (app / '.forklaunch/manifest.toml').read_text()
        return re.findall(r'^(\[\[?[^\]]+\]\]?|[A-Za-z_]+)\s*(?:=|$)', text, re.M)
    with case('sync all keeps manifest order'):
        before = layout()
        run([cli, 'sync', 'all', '-c', '--path', app])
        assert layout() == before, f'sync all reordered the manifest:\n{before}\n{layout()}'

    # Application: rename reaches Compose, .env and package scopes; license keeps a LICENSE file.
    with case('change application --name'):
        forklaunch('change', 'application', '-N', 'kestrel', '-c')
        for path in ['docker-compose.yaml', '.env', 'src/accounts/.env.local', 'src/accounts/sdk/typescript/package.json']:
            assert not re.search(r'\bzebra[-_/]', (app / path).read_text()), f'{path} keeps the old application name'
        holder = next(line for line in (app / 'src/LICENSE').read_text().splitlines() if line.startswith('Copyright'))
        assert holder.split()[-1] == 'kestrel', f'LICENSE keeps the old application name: {holder}'
    with case('change application --license'):
        forklaunch('change', 'application', '-L', 'Apache-2.0', '-c')
        assert 'Apache License' in (app / 'src/LICENSE').read_text()
    with case('change application --author'):
        forklaunch('change', 'application', '-A', 'Jane Doe', '-c')
        assert '"Jane Doe"' in (app / 'Cargo.toml').read_text()

    # Service rename keeps its database wiring and port.
    with case('change service --name (database wiring, port)'):
        before = json.loads(run(['docker', 'compose', 'config', '--no-interpolate', '--format', 'json']).stdout)['services']['accounts']
        forklaunch('change', 'service', '-N', 'ledgers', '-c', path=app / 'src/accounts')
        after = json.loads(run(['docker', 'compose', 'config', '--no-interpolate', '--format', 'json']).stdout)['services']['ledgers']
        assert after['environment']['DATABASE_URL'] == '${LEDGERS_DATABASE_URL:?Configure a restricted service PostgreSQL role}'
        assert after['ports'] == before['ports'] and 'ledgers-migrate' in after['depends_on']
        stale('accounts')
    with case('change router --name'):
        forklaunch('change', 'router', '-e', 'ledger', '-N', 'journal', '-c', path=app / 'src/ledgers')

    # Worker backends cycle back to the starting point; a worker converts to a service and back.
    original = (app / 'src/dbjobs/consumer/worker.rs').read_text() if (app / 'src/dbjobs/consumer/worker.rs').exists() else None
    with case('change worker --type redis -> kafka -> database'):
        for backend in ['redis', 'kafka', 'database']:
            forklaunch('change', 'worker', '-t', backend, *(['-d', 'postgresql'] if backend == 'database' else []), '-c', path=app / 'src/dbjobs')
        assert (app / 'src/dbjobs/consumer/worker.rs').read_text() == original
    with case('change worker --to service, service --to worker'):
        forklaunch('change', 'worker', '--to', 'service', '-c', path=app / 'src/dbjobs')
        assert not (app / 'src/dbjobs/consumer').exists()
        forklaunch('change', 'service', '--to', 'worker', '-t', 'database', '-c', path=app / 'src/dbjobs')
        assert (app / 'src/dbjobs/consumer/worker.rs').read_text() == original

    with case('change worker --name'):
        forklaunch('change', 'worker', '-N', 'batches', '-c', path=app / 'src/dbjobs')
        stale('dbjobs')
    with case('change library --name'):
        forklaunch('change', 'library', '-N', 'common', '-c', path=app / 'src/core/shared')
        stale('shared')
    with case('change service --name (module)'):
        forklaunch('change', 'service', '-N', 'identity', '-c', path=app / 'src/iam')
        stale('iam', renamed='identity')

    if args.group != 'delete' and not args.skip_cargo:
        with case('cargo build after changes'):
            run(['cargo', 'build', '--workspace'])
        with case('cargo clippy after changes'):
            run(['cargo', 'clippy', '--workspace', '--all-targets', '--', '-D', 'warnings'])

    if args.group != 'change':
        # Deletes leave nothing behind; the last Kafka reader takes the broker with it.
        with case('delete router'):
            run([cli, 'delete', 'router', 'journal', '-c', '--path', app / 'src/ledgers'])
        for kind, name in [('service', 'ledgers'), ('worker', 'batches'), ('worker', 'events'), ('library', 'common'), ('service', 'helper')]:
            with case(f'delete {kind} {name}'):
                run([cli, 'delete', kind, name, '-c', '--path', app])
                run(['docker', 'compose', 'config', '-q'])
                stale(name)
        with case('delete last Kafka worker removes the broker'):
            compose = json.loads(run(['docker', 'compose', 'config', '--no-interpolate', '--format', 'json']).stdout)['services']
            assert 'kafka' not in compose, 'Kafka outlived its last worker'
        if not args.skip_cargo:
            with case('cargo clippy after deletes'):
                run(['cargo', 'clippy', '--workspace', '--all-targets', '--', '-D', 'warnings'])
        with case('delete service (module)'):
            run([cli, 'delete', 'service', 'identity', '-c', '--path', app])
            stale('identity')
    passed = report()
    print(('PASS' if passed else 'FAIL') + f' Rust change/delete command sweep ({args.group})', flush=True)
    raise SystemExit(0 if passed else 1)
finally:
    if args.keep:
        print('Fixture retained:', root)
    else:
        shutil.rmtree(root, ignore_errors=True)

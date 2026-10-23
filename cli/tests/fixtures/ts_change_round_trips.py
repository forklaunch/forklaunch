#!/usr/bin/env python3
"""TypeScript code-editing commands undo themselves byte for byte.

Scaffolds a Node application with one PostgreSQL service, commits it, and
checks that each change and its inverse (and each add and its delete) returns
the working tree to that commit: no leftover files, keys, dependencies or
scripts, and no reordered ones.

  - change application -r bun / -r node, and -N (which also renames the
    LICENSE copyright holder);
  - infra add / remove of the cache and the object store;
  - init service / worker on another database (sqlite and better-sqlite
    included), then delete; the generated production image lists the new
    service while it exists;
  - change service -d to another database and back (mikro-orm.config.ts,
    test-utils.ts, the core base entity and Compose included);
  - sync all leaves the manifest byte for byte;
  - the manifest never gains a top-level snake_case_name, and the MIT
    license names the year.

No package installs; only the CLI runs. Set FORKLAUNCH_CLI to a built binary.
"""
import argparse
import os
import re
from pathlib import Path
import shutil
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--keep', action='store_true')
args = parser.parse_args()
repo = Path(__file__).resolve().parents[3]
cli = Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/release/forklaunch')).resolve()
env = {**os.environ, 'FORKLAUNCH_SKIP_PRECHECK': '1'}
root = Path(tempfile.mkdtemp(prefix='forklaunch-ts-round-trips-'))
base = root / 'base'


def run(arguments, cwd, check=True):
    result = subprocess.run([str(a) for a in arguments], cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                            text=True, capture_output=True)
    if check:
        assert result.returncode == 0, ' '.join(map(str, arguments)) + '\n' + result.stdout[-3000:] + result.stderr[-3000:]
    return result


def git(*arguments, cwd):
    return run(['git', *arguments], cwd).stdout


def round_trip(label, *commands, check=None):
    """Run `commands` (each a list of CLI arguments) on a copy of the base and
    require the tree to equal the base commit afterwards."""
    app = root / label
    shutil.copytree(base, app, symlinks=True)
    for command in commands:
        run([cli, *command], app)
        if check:
            check(app, command)
    status = git('status', '--porcelain', '--untracked-files=all', cwd=app)
    assert not status, f'{label} did not round-trip:\n{status}\n' + git('diff', cwd=app)[:6000]
    print(f'PASS {label}', flush=True)


def no_stray_manifest_keys(app, _command):
    manifest = (app / '.forklaunch/manifest.toml').read_text()
    assert not any(line.startswith('snake_case_name') for line in manifest.splitlines()), manifest[:400]


try:
    run([cli, 'init', 'application', 'zebra', '--path', base, '-d', 'postgresql', '-v', 'zod', '-f', 'prettier',
         '-l', 'oxlint', '-F', 'express', '-r', 'node', '-t', 'vitest', '-D', 'Round trips', '-A', 'ForkLaunch',
         '-L', 'MIT', '-o', 'src'], root)
    run([cli, 'init', 'service', 'accounts', '-d', 'postgresql', '-D', 'Accounts', '--path', '.'], base)
    no_stray_manifest_keys(base, None)
    holder = next(line for line in (base / 'src/LICENSE').read_text().splitlines() if line.startswith('Copyright'))
    assert re.fullmatch(r'Copyright \(c\) \d{4} zebra', holder), holder
    assert 'src/accounts/' in (base / 'Dockerfile').read_text(), 'init service did not refresh the generated image'
    git('init', '-q', cwd=base)
    git('add', '-A', cwd=base)
    git('-c', 'user.email=ci@forklaunch.com', '-c', 'user.name=ci', 'commit', '-qm', 'base', cwd=base)

    def bun_leftovers(app, command):
        if 'bun' in command:
            assert (app / 'src/bunfig.toml').exists() and '"overrides"' in (app / 'src/package.json').read_text()

    round_trip('runtime', ['change', 'application', '-r', 'bun', '-c', '--path', '.'],
               ['change', 'application', '-r', 'node', '-c', '--path', '.'], check=bun_leftovers)

    def license_holder(app, command):
        name = command[command.index('-N') + 1]
        holder = next(line for line in (app / 'src/LICENSE').read_text().splitlines() if line.startswith('Copyright'))
        assert holder.split()[-1] == name, holder

    round_trip('rename', ['change', 'application', '-N', 'kestrel', '-c', '--path', '.'],
               ['change', 'application', '-N', 'zebra', '-c', '--path', '.'], check=license_holder)

    for resource in ['cache', 'object_store']:
        round_trip(f'infra-{resource}', ['infra', 'add', 'accounts', resource, '--path', '.'],
                   ['infra', 'remove', 'accounts', resource, '--path', '.'])

    def core_dependencies(app, command):
        core = (app / 'src/core/package.json').read_text()
        assert core.count('"@mikro-orm/migrations"') <= 1, core
        no_stray_manifest_keys(app, command)
        if command[:2] == ['init', 'service']:
            assert f'src/{command[2]}/' in (app / 'Dockerfile').read_text(), 'generated image not refreshed'

    for database in ['mysql', 'mongodb', 'postgresql', 'sqlite', 'better-sqlite']:
        round_trip(f'service-{database}', ['init', 'service', 'ledger', '-d', database, '-D', 'Ledger', '--path', '.'],
                   ['delete', 'service', 'ledger', '-c', '--path', '.'], check=core_dependencies)
    round_trip('worker-mysql', ['init', 'worker', 'jobs', '-t', 'database', '-d', 'mysql', '-D', 'Jobs', '--path', '.'],
               ['delete', 'worker', 'jobs', '-c', '--path', '.'], check=core_dependencies)
    for database in ['mysql', 'mongodb', 'sqlite']:
        round_trip(f'change-database-{database}', ['change', 'service', '-d', database, '-c', '--path', 'src/accounts'],
                   ['change', 'service', '-d', 'postgresql', '-c', '--path', 'src/accounts'])

    # sync all may add env settings, but the manifest stays as written.
    synced = root / 'sync'
    shutil.copytree(base, synced, symlinks=True)
    run([cli, 'sync', 'all', '-c', '--path', '.'], synced)
    assert not git('status', '--porcelain', '--', '.forklaunch/manifest.toml', cwd=synced), git('diff', cwd=synced)[:3000]
    print('PASS sync-manifest', flush=True)
    print('PASS TypeScript change round trips', flush=True)
finally:
    if args.keep:
        print('Fixture retained:', root)
    else:
        shutil.rmtree(root, ignore_errors=True)

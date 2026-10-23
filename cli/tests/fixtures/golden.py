#!/usr/bin/env python3
"""Golden applications: canonical apps scaffolded and built once, then reused.

Most end-to-end scripts in cli/tests scaffold an application only to get a
realistic starting point, then compile it, install it or inspect it. Building
the same dependency graph again in every script dominated CI time. This module
scaffolds a handful of canonical applications with the CLI under test, builds
them once into a stable directory, and hands consumers either a copy of an app
or the shared Cargo target directory its dependencies were compiled into.

Layout of a golden directory (``FORKLAUNCH_GOLDEN_APPS_DIR``)::

    <dir>/golden.json          stamp: CLI hash, spec hashes, toolchain, apps built
    <dir>/<app>/scaffold/      the application exactly as the CLI steps left it
    <dir>/<app>/app/           the same application after its build steps
    <dir>/target/              Cargo target shared by every Rust golden app

Consumers (Python)::

    from fixtures import golden
    app = golden.app('ts-node', root / 'app')         # built copy (node_modules, dist)
    app = golden.app('rust-service', root / 'app', built=False)  # pristine scaffold
    env['CARGO_TARGET_DIR'] = env.get('CARGO_TARGET_DIR') or golden.target()

Consumers (shell): ``source fixtures/golden.sh`` then ``golden_app NAME DEST``.

When ``FORKLAUNCH_GOLDEN_APPS_DIR`` is set the apps must already be there (CI
unpacks the artifact); a missing app or a stamp built by another CLI fails
loudly. Without it the apps are built on demand under cli/tests/output/golden
and rebuilt whenever the CLI binary or an app's definition changes.

Entry point: cli/tests/golden_apps.sh (build | package | unpack | list | path).
"""
from __future__ import annotations

import fcntl
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tarfile
import time

REPO = Path(__file__).resolve().parents[3]
TESTS = REPO / 'cli/tests'
DEFAULT_DIR = TESTS / 'output/golden'
ENV_DIR = 'FORKLAUNCH_GOLDEN_APPS_DIR'
SCHEMA = 2

# The TypeScript shell most Rust runtime scripts start from.
TS_SHELL = ['--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier',
            '--linter', 'oxlint', '--http-framework', 'express', '--runtime', 'node',
            '--test-framework', 'vitest']

# Each app: CLI steps (run in order with --path pointing at the app, or at a
# path relative to it), build steps (run in a directory relative to the app),
# whether generated files are formatted, and the bundle it ships in. Names
# are neutral unless a consumer asserts on them (see 'mixed').
APPS: dict[str, dict] = {
    # The canonical TypeScript application of the init/eject family, node half:
    # express, zod, prettier, billing-base and iam-base, installed and built.
    'ts-node': {
        'bundle': 'ts',
        'format': True,
        'steps': [
            ['init', 'application', 'golden-ts-node', '-o', 'src/modules', '-d', 'postgresql',
             '-f', 'prettier', '-l', 'oxlint', '-v', 'zod', '-F', 'express', '-r', 'node',
             '-t', 'vitest', '-m', 'billing-base', '-m', 'iam-base', '-D', 'Test service',
             '-A', 'Rohin Bhargava', '-L', 'AGPL-3.0'],
        ],
        'build': [
            ('src/modules', ['pnpm', 'install']),
            ('src/modules', ['pnpm', 'build']),
        ],
    },
    # The same application on Bun with Biome, installed and built.
    'ts-bun': {
        'bundle': 'ts',
        'format': True,
        'steps': [
            ['init', 'application', 'golden-ts-bun', '-o', 'src/modules', '-d', 'postgresql',
             '-f', 'biome', '-l', 'oxlint', '-v', 'zod', '-F', 'express', '-r', 'bun',
             '-t', 'vitest', '-m', 'billing-base', '-m', 'iam-base', '-D', 'Test service',
             '-A', 'Rohin Bhargava', '-L', 'MIT'],
        ],
        'build': [
            ('src/modules', ['bun', 'install']),
            ('src/modules', ['bun', 'run', 'build']),
        ],
    },
    # TypeScript shell plus one in-memory Rust service: the starting point of
    # the HTTP, SSE, socket, SDK, container and telemetry runtime scripts.
    'rust-service': {
        'bundle': 'rust',
        'format': False,
        'steps': [
            ['init', 'application', 'golden-rust-service', *TS_SHELL,
             '-D', 'Golden Rust service', '-A', 'ForkLaunch', '-L', 'MIT'],
            ['init', 'service', 'accounts', '--language', 'rust', '-D', 'Accounts'],
        ],
        'build': [('.', ['cargo', 'build', '--workspace'])],
    },
    # A Rust-language application with every component kind, built as one
    # workspace. Cargo unifies features across the packages it builds, so this
    # warms the crates whose features do not vary between apps (most of the
    # graph); a script's exact feature set is only warm in an app of its shape.
    'rust-full': {
        'bundle': 'rust',
        'format': False,
        'steps': [
            ['init', 'application', 'golden-rust-full', '--language', 'rust',
             '-D', 'Golden Rust application', '-A', 'ForkLaunch', '-L', 'MIT'],
            ['init', 'service', 'api', '--language', 'rust', '-D', 'API'],
            ['init', 'router', 'users', '--language', 'rust', ('path', 'src/api')],
            ['init', 'service', 'records', '--language', 'rust', '-d', 'postgresql', '-D', 'Records'],
            ['init', 'service', 'notes', '--language', 'rust', '--mappers',
             '--infrastructure', 'redis', 's3', '-D', 'Notes'],
            ['init', 'worker', 'jobs', '--language', 'rust', '-t', 'database', '-d', 'postgresql',
             '--runtime', 'apalis', '-D', 'Jobs'],
            ['init', 'worker', 'queue', '--language', 'rust', '--consumer-only', '-t', 'database',
             '-d', 'postgresql', '-D', 'Queue'],
            ['init', 'worker', 'tasks', '--language', 'rust', '-t', 'redis', '-D', 'Tasks'],
            ['init', 'worker', 'inbox', '--language', 'rust', '--consumer-only', '-t', 'redis',
             '-D', 'Inbox'],
            ['init', 'worker', 'events', '--language', 'rust', '--consumer-only', '-t', 'kafka',
             '-D', 'Events'],
            ['init', 'library', 'shared', '--language', 'rust', '-D', 'Shared'],
            ['init', 'library', 'clients', '--language', 'rust', '--sdk-services', 'api',
             '-D', 'Clients'],
            ['init', 'module', 'iam', '--language', 'rust', '-m', 'iam-base', '-d', 'postgresql'],
            ['init', 'module', 'billing', '--language', 'rust', '-m', 'billing-base'],
            ['init', 'module', 'payments', '--language', 'rust', '-m', 'billing-stripe'],
            ['init', 'module', 'cac', '--language', 'rust', '-m', 'cac-base'],
            ['init', 'module', 'ecommerce', '--language', 'rust', '-m', 'ecommerce-stripe'],
            ['init', 'agent', 'assistant', '--language', 'rust', '-D', 'Assistant'],
        ],
        'build': [('.', ['cargo', 'build', '--workspace'])],
    },
    # TypeScript-first mixed workspace: a TypeScript service beside a native
    # Rust service, the starting point of the mixed SDK workspace suites.
    'mixed': {
        'bundle': 'rust',
        'format': False,
        'steps': [
            # Named as rust_sdk_workspace.py names its node fixture: it asserts
            # the generated @node-workspace/* package names.
            ['init', 'application', 'node-workspace', '--modules-path', 'modules', *TS_SHELL,
             '-D', 'SDK workspace fixture', '-A', 'ForkLaunch', '-L', 'MIT'],
            ['init', 'service', 'legacy-api', '--language', 'typescript', '-d', 'postgresql',
             '-D', 'Legacy API'],
            ['init', 'service', 'native-api', '--language', 'rust', '-D', 'Native API'],
        ],
        'build': [('.', ['cargo', 'build', '-p', 'native-api'])],
    },
}

BUNDLES = {'ts': ['ts-node', 'ts-bun'], 'rust': ['rust-service', 'rust-full', 'mixed']}


def log(*parts):
    print('[golden]', *parts, file=sys.stderr, flush=True)


def directory() -> Path:
    configured = os.environ.get(ENV_DIR)
    return Path(configured).resolve() if configured else DEFAULT_DIR


def artifact_mode() -> bool:
    return bool(os.environ.get(ENV_DIR))


def cli_binary() -> Path:
    configured = os.environ.get('FORKLAUNCH_CLI') or os.environ.get('FORKLAUNCH_TEST_CLI')
    if configured and Path(configured).is_file():
        return Path(configured).resolve()
    # Scripts fall back to `cargo run` of the checkout; use its newest build.
    candidates = [REPO / 'cli/target' / profile / 'forklaunch' for profile in ('debug', 'release')]
    candidates = [c for c in candidates if c.is_file()]
    if candidates:
        return max(candidates, key=lambda c: c.stat().st_mtime)
    raise SystemExit('[golden] no CLI binary: set FORKLAUNCH_CLI or build cli/ first')


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open('rb') as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b''):
            digest.update(chunk)
    return digest.hexdigest()


def spec_hash(name: str) -> str:
    spec = APPS[name]
    material = json.dumps({'schema': SCHEMA, 'steps': spec['steps'], 'build': spec['build'],
                           'format': spec['format']}, sort_keys=True, default=str)
    return hashlib.sha256(material.encode()).hexdigest()[:16]


def target() -> str:
    """The Cargo target directory every Rust golden app was compiled into."""
    return str(directory() / 'target')


def build_env(spec: dict) -> dict:
    env = {**os.environ, 'CARGO_INCREMENTAL': '0', 'RUSTUP_TOOLCHAIN': 'stable',
           'CARGO_TARGET_DIR': target(), 'RUST_BACKTRACE': '1'}
    if spec['format']:
        env.pop('FORKLAUNCH_SKIP_FORMAT', None)
    else:
        env['FORKLAUNCH_SKIP_FORMAT'] = '1'
    return env


def read_stamp(base: Path) -> dict:
    try:
        return json.loads((base / 'golden.json').read_text())
    except (OSError, ValueError):
        return {}


def write_stamp(base: Path, stamp: dict) -> None:
    temporary = base / 'golden.json.tmp'
    temporary.write_text(json.dumps(stamp, indent=2, sort_keys=True) + '\n')
    temporary.replace(base / 'golden.json')


# Settings that change every Cargo fingerprint: consumers must match the build.
FINGERPRINT_ENV = ('CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_DEV_OPT_LEVEL', 'RUSTFLAGS',
                   'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_TARGET')


def cargo_env() -> dict:
    return {key: os.environ[key] for key in FINGERPRINT_ENV if os.environ.get(key)}


def toolchain() -> str:
    try:
        return subprocess.run(['rustc', '-V'], capture_output=True, text=True, timeout=60,
                              env={**os.environ, 'RUSTUP_TOOLCHAIN': 'stable'}).stdout.strip()
    except OSError:
        return ''


def run(argv, cwd: Path, env: dict, timeout=3600):
    log('$', shlex.join(map(str, argv)), f'(in {cwd})')
    subprocess.run(list(map(str, argv)), cwd=cwd, env=env, check=True, timeout=timeout)


def build_app(name: str, base: Path, cli: Path) -> None:
    spec = APPS[name]
    home = base / name
    app = home / 'app'
    if home.exists():
        shutil.rmtree(home)
    home.mkdir(parents=True)
    env = build_env(spec)
    started = time.time()
    for step in spec['steps']:
        argv, path = [], app
        for part in step:
            if isinstance(part, tuple) and part[0] == 'path':
                path = app / part[1]
            else:
                argv.append(part)
        run([cli, *argv, '--path', path], cwd=home, env=env, timeout=600)
    copy_tree(app, home / 'scaffold')
    for relative, command in spec['build']:
        run(command, cwd=app / relative, env=env)
    log(f'{name}: built in {time.time() - started:.0f}s')


class _Lock:
    def __init__(self, base: Path):
        base.mkdir(parents=True, exist_ok=True)
        self.handle = (base / '.lock').open('a')

    def __enter__(self):
        fcntl.flock(self.handle, fcntl.LOCK_EX)
        return self

    def __exit__(self, *exc):
        fcntl.flock(self.handle, fcntl.LOCK_UN)
        self.handle.close()


def build(names: list[str], base: Path | None = None, force: bool = False) -> Path:
    """Scaffold and build the named apps (missing or stale ones only, unless force)."""
    base = base or directory()
    cli = cli_binary()
    cli_sha = sha256(cli)
    with _Lock(base):
        stamp = read_stamp(base)
        if stamp.get('cli_sha256') != cli_sha or stamp.get('schema') != SCHEMA:
            stamp = {'schema': SCHEMA, 'cli_sha256': cli_sha, 'apps': {}}
        stamp['toolchain'] = toolchain()
        stamp['cargo_env'] = cargo_env()
        for name in names:
            if name not in APPS:
                raise SystemExit(f'[golden] unknown app {name!r}; known: {", ".join(APPS)}')
            current = stamp['apps'].get(name, {})
            if (not force and current.get('spec') == spec_hash(name) and (base / name / 'app').is_dir()
                    and (base / name / 'scaffold').is_dir()):
                log(f'{name}: up to date')
                continue
            stamp['apps'].pop(name, None)
            write_stamp(base, stamp)
            build_app(name, base, cli)
            stamp['apps'][name] = {'spec': spec_hash(name), 'built_at': int(time.time())}
            write_stamp(base, stamp)
    return base


def ensure(name: str, built: bool = True) -> Path:
    """Path of a ready golden app, building it locally when not in artifact mode."""
    base = directory()
    leaf = 'app' if built else 'scaffold'
    stamp = read_stamp(base)
    entry = stamp.get('apps', {}).get(name)
    ready = (entry is not None and entry.get('spec') == spec_hash(name)
             and (base / name / 'app').is_dir() and (base / name / 'scaffold').is_dir())
    cli_matches = stamp.get('cli_sha256') == sha256(cli_binary())
    if artifact_mode():
        if not ready:
            raise SystemExit(f'[golden] {ENV_DIR}={base} has no current {name!r}; '
                             'unpack the golden-apps artifact or unset the variable to build locally')
        if not cli_matches and os.environ.get('FORKLAUNCH_GOLDEN_ALLOW_CLI_MISMATCH') != '1':
            raise SystemExit(f'[golden] {base} was scaffolded by a different CLI than FORKLAUNCH_CLI')
        if stamp.get('cargo_env', {}) != cargo_env():
            log(f'warning: built with {stamp.get("cargo_env", {})}, this job has {cargo_env()}: '
                'the shared target will not be warm')
        return base / name / leaf
    if not (ready and cli_matches):
        build([name], base)
    return base / name / leaf


def copy_tree(source: Path, destination: Path) -> None:
    """Copy keeping symlinks, modes and mtimes (pnpm links, Cargo fingerprints)."""
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        raise SystemExit(f'[golden] refusing to overwrite {destination}')
    flags = ['-c'] if sys.platform == 'darwin' else ['--reflink=auto']
    result = subprocess.run(['cp', '-a', *flags, str(source), str(destination)], capture_output=True, text=True)
    if result.returncode != 0:
        subprocess.run(['cp', '-a', str(source), str(destination)], check=True)


def app(name: str, destination: Path | str, built: bool = True) -> Path:
    """A fresh, mutable copy of golden app `name` at `destination`.

    built=True copies the app after its build steps (installed node_modules,
    compiled dist, build.rs outputs); built=False copies the pristine scaffold,
    for scripts whose assertions depend on running the first build themselves.
    """
    destination = Path(destination).resolve()
    copy_tree(ensure(name, built), destination)
    log(f'{name} -> {destination}')
    return destination


def package(bundle: str, output: Path, base: Path | None = None) -> Path:
    """Write one bundle (its apps, plus the shared target for Rust) as a tarball."""
    base = base or directory()
    names = BUNDLES[bundle]
    stamp = read_stamp(base)
    missing = [n for n in names if n not in stamp.get('apps', {})]
    if missing:
        raise SystemExit(f'[golden] cannot package {bundle}: not built: {", ".join(missing)}')
    output.parent.mkdir(parents=True, exist_ok=True)
    members = ['golden.json', *names]
    if bundle == 'rust' and (base / 'target').is_dir():
        members.append('target')
    excluded = ['target/debug/incremental', 'target/release/incremental', 'target/tmp',
                'target/debug/examples']
    if 'target' in members:
        excluded += member_outputs(base, names)
    compress = ['--zstd'] if shutil.which('zstd') else ['-z']
    argv = ['tar', '-C', str(base), *compress, '-cf', str(output),
            *[f'--exclude={pattern}' for pattern in excluded], *members]
    log(f'$ tar -C {base} -cf {output} {" ".join(members)} ({len(excluded)} exclusions)')
    subprocess.run(argv, check=True)
    log(f'{output}: {output.stat().st_size / 1e6:.0f} MB')
    return output


def member_outputs(base: Path, names: list[str]) -> list[str]:
    """Target entries of the golden apps' own crates.

    A consumer copies an app to another path, which gives its crates new
    package ids and so new output hashes: the golden apps' own binaries and
    libraries are never reused, only their dependencies are. Leaving them out
    keeps the artifact to what consumers can use.
    """
    crates = set()
    for name in names:
        app = base / name / 'app'
        if not (app / 'Cargo.toml').is_file():
            continue
        metadata = json.loads(subprocess.run(
            ['cargo', 'metadata', '--no-deps', '--format-version', '1', '--offline'], cwd=app,
            capture_output=True, text=True, check=True, timeout=300,
            env={**os.environ, 'RUSTUP_TOOLCHAIN': 'stable'}).stdout)
        crates.update(package['name'] for package in metadata['packages'])
    patterns = []
    for crate in sorted(crates):
        lib = crate.replace('-', '_')
        for stem in sorted({crate, lib}):
            patterns += [f'target/debug/deps/{stem}-*', f'target/debug/deps/lib{stem}-*',
                         f'target/debug/.fingerprint/{stem}-*', f'target/debug/build/{stem}-*',
                         f'target/debug/{stem}', f'target/debug/{stem}.d', f'target/debug/lib{stem}.*']
    return patterns


def unpack(archives: list[Path], base: Path | None = None) -> Path:
    """Extract bundles into the golden directory, merging their stamps."""
    base = base or directory()
    base.mkdir(parents=True, exist_ok=True)
    merged = read_stamp(base)
    for archive in archives:
        compress = ['--zstd'] if str(archive).endswith('.zst') else ['-z'] if str(archive).endswith('gz') else []
        subprocess.run(['tar', '-C', str(base), *compress, '-xpf', str(archive)], check=True)
        stamp = read_stamp(base)
        if merged and merged.get('cli_sha256') not in (None, stamp.get('cli_sha256')):
            raise SystemExit(f'[golden] {archive} was built by a different CLI than the other bundles')
        apps = {**merged.get('apps', {}), **stamp.get('apps', {})}
        merged = {**stamp, 'apps': apps}
        write_stamp(base, merged)
    log(f'unpacked {len(archives)} bundle(s) into {base}: {", ".join(sorted(merged.get("apps", {})))}')
    return base


def main(argv: list[str]) -> int:
    import argparse
    parser = argparse.ArgumentParser(prog='golden_apps.sh', description=__doc__.split('\n')[0])
    sub = parser.add_subparsers(dest='command', required=True)
    p = sub.add_parser('build', help='scaffold and build golden apps')
    p.add_argument('apps', nargs='*', help='app or bundle names (default: all)')
    p.add_argument('--force', action='store_true')
    p = sub.add_parser('package', help='write bundle tarballs')
    p.add_argument('--out', required=True, type=Path, help='directory for golden-apps-<bundle>.tar.*')
    p.add_argument('bundles', nargs='*', default=list(BUNDLES))
    p = sub.add_parser('unpack', help='extract bundle tarballs into the golden directory')
    p.add_argument('archives', nargs='+', type=Path)
    sub.add_parser('list', help='show apps, bundles and what is built')
    p = sub.add_parser('path', help='print the path of a ready app (building locally if needed)')
    p.add_argument('app')
    p.add_argument('--scaffold', action='store_true')
    p = sub.add_parser('copy', help='copy a ready app to DEST')
    p.add_argument('app')
    p.add_argument('dest', type=Path)
    p.add_argument('--scaffold', action='store_true', help='copy the pristine scaffold, not the built app')
    sub.add_parser('target', help='print the shared Cargo target directory')
    args = parser.parse_args(argv)
    if args.command == 'build':
        names = []
        for item in args.apps or list(BUNDLES):
            names += BUNDLES.get(item, [item])
        build(list(dict.fromkeys(names)), force=args.force)
    elif args.command == 'package':
        extension = 'tar.zst' if shutil.which('zstd') else 'tar.gz'
        for bundle in args.bundles:
            package(bundle, args.out.resolve() / f'golden-apps-{bundle}.{extension}')
    elif args.command == 'unpack':
        unpack([a.resolve() for a in args.archives])
    elif args.command == 'list':
        stamp = read_stamp(directory())
        print(f'directory: {directory()} ({"artifact" if artifact_mode() else "local"} mode)')
        for bundle, names in BUNDLES.items():
            for name in names:
                state = 'built' if name in stamp.get('apps', {}) else 'missing'
                print(f'  {bundle:5} {name:13} {state}')
    elif args.command == 'path':
        print(ensure(args.app, built=not args.scaffold))
    elif args.command == 'copy':
        print(app(args.app, args.dest, built=not args.scaffold))
    elif args.command == 'target':
        print(target())
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))

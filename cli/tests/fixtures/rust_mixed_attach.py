#!/usr/bin/env python3
"""Attach native projects to raw Node/Bun scaffolds and run the relocated servers."""
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import tomllib

repo = Path(__file__).resolve().parents[3]
binary = Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch')).resolve()
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'CARGO_INCREMENTAL': '0',
       'RUSTUP_TOOLCHAIN': 'stable'}
env.setdefault('CARGO_TARGET_DIR', str(repo / 'cli/target'))
if not env.get('FORKLAUNCH_WORKER_RUNTIME_LOCKED'):
    sys.exit(subprocess.run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'),
        '-c', shlex.join([sys.executable, str(Path(__file__).resolve())])], env=env).returncode)

with binary.open('rb') as executable:
    print('CLI SHA256:', hashlib.file_digest(executable, 'sha256').hexdigest(), flush=True)
output = repo / 'cli/tests/output/rust-mixed-attach'
output.mkdir(parents=True, exist_ok=True)
root = Path(tempfile.mkdtemp(prefix='run.', dir=output)).resolve()
app = root / 'node'

def rust_image():
    """The generated Rust image inventory, read from Dockerfile.rust and Compose."""
    lines = (app / 'Dockerfile.rust').read_text().splitlines()
    inputs = [json.loads(line[len('COPY '):])[0] for line in lines if line.startswith('COPY [')]
    release = 'COPY --from=builder /app/target/release/'
    binaries = [line[len(release):].split(' ')[0] for line in lines if line.startswith(release)]
    services = json.loads(subprocess.check_output(['docker', 'compose', '-f', str(app / 'docker-compose.yaml'),
        'config', '--no-interpolate', '--format', 'json'], cwd=app, text=True))['services']
    # Long-running processes; one-shot migrate/seed jobs run the same binary
    # with --migrate/--seed and are not processes.
    processes = {name: service for name, service in services.items()
                 if (service.get('command') or [''])[0].startswith('/app/bin/')
                 and not set(service['command'][1:]) & {'--migrate', '--seed'}}
    return {'inputs': inputs, 'binaries': binaries, 'processes': processes}

custom_dockerfile = '# Existing custom TypeScript image\nFROM node:22\n'


def run(*arguments, success=True):
    command = [str(binary), *arguments, '--path', str(app)]
    print('Running:', shlex.join(command), flush=True)
    result = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True, timeout=300)
    print(result.stdout, end='', flush=True)
    if success:
        assert result.returncode == 0, result.stderr
    else:
        assert result.returncode != 0, result.stdout
    return result


def execute(arguments, *, timeout=1800, extra_env=None):
    print('Running:', shlex.join(map(str, arguments)), flush=True)
    subprocess.run(arguments, cwd=app, env={**env, **(extra_env or {})}, check=True, timeout=timeout)


def snapshot(directory=None):
    return {str(path.relative_to(app)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in (directory or app).rglob('*') if path.is_file()}


def typescript_sources():
    # SDK declarations/barrels and workspace manifests are generated ownership
    # boundaries. Existing ordinary TS sources/configuration must stay byte-for-byte,
    # except that the shared production image excludes its `production` build
    # output from each service's compiler config (typescript_image.rs).
    def digest(relative, value):
        # A native backend adds the settings TypeScript consumers need to reach
        # it to their .env.local, and mixed apps point host database tooling at
        # each service's owner role (8c25eaee8); every other line survives.
        if Path(relative).name == '.env.local':
            return frozenset(line for line in (app / relative).read_text().splitlines() if not line.startswith('DB_USER='))
        if Path(relative).name != 'tsconfig.json':
            return value
        config = json.loads((app / relative).read_text())
        if isinstance(config.get('exclude'), list):
            config['exclude'] = [entry for entry in config['exclude'] if entry != 'production']
        return hashlib.sha256(json.dumps(config, sort_keys=True).encode()).hexdigest()
    return {relative: digest(relative, value) for relative, value in snapshot(app / 'modules').items()
            if Path(relative).name != 'package.json'
            and relative != 'modules/pnpm-workspace.yaml'
            and not relative.startswith(('modules/client-sdk/', 'modules/.forklaunch/'))
            and '/sdk/' not in relative}


def preserve_sources(original):
    current = typescript_sources()
    kept = lambda path, content: content <= current.get(path, frozenset()) if isinstance(content, frozenset) else current.get(path) == content
    assert all(kept(path, content) for path, content in original.items()), [
        path for path, content in original.items() if not kept(path, content)]
    assert (app / 'Dockerfile').read_text() == custom_dockerfile
    for package_path in ['modules/package.json', 'modules/legacy-api/package.json']:
        package = json.loads((app / package_path).read_text())
        assert package['scripts']['user:owned'] == 'echo preserved'
        assert package['applicationOwned'] == {'owner': 'typescript-first-fixture'}
    assert (app / 'modules/client-sdk/authored-extension.ts').read_text() == 'export const authoredExtension = 42;\n'


def compose():
    return json.loads(subprocess.check_output(['docker', 'compose', '-f', str(app / 'docker-compose.yaml'),
        'config', '--no-interpolate', '--format', 'json'], cwd=app, env=env, text=True))


try:
    for runtime in ['node', 'bun']:
        app = root / runtime
        run('init', 'application', runtime + '-mixed', '--modules-path', 'modules',
            '--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint',
            '--http-framework', 'express', '--runtime', runtime, '--test-framework', 'vitest',
            '-D', 'Attach native backend', '-A', 'ForkLaunch', '-L', 'MIT')
        run('init', 'service', 'legacy-api', '--language', 'typescript', '-d', 'postgresql', '-D', 'Legacy API')
        authored = app / 'modules/legacy-api/domain/services/attachment-proof.ts'
        authored.parent.mkdir(parents=True, exist_ok=True)
        authored.write_text('export const applicationOwned = "preserved";\n')
        (app / 'modules/client-sdk/authored-extension.ts').write_text('export const authoredExtension = 42;\n')
        for package_path in ['modules/package.json', 'modules/legacy-api/package.json']:
            path = app / package_path
            package = json.loads(path.read_text())
            package.setdefault('scripts', {})['user:owned'] = 'echo preserved'
            package['applicationOwned'] = {'owner': 'typescript-first-fixture'}
            path.write_text(json.dumps(package, indent=2) + '\n')
        original = typescript_sources()
        before = snapshot()
        run('init', 'service', 'native-api', '--language', 'rust', '-D', 'Native API', '--dryrun')
        assert snapshot() == before, 'Dry run changed the TypeScript application'
        assert not (app / 'modules/native-api').exists()

        # Editing an owned image must reject before writes; an unrelated custom
        # Dockerfile is preserved once the generated image has its language name.
        managed_image = app / 'Dockerfile'
        generated_image = managed_image.read_bytes()
        # Ownership is recorded only in the file's generation stamp (d3fe042d1):
        # an edit keeps the stamp, while a file without one is a custom image.
        managed_image.write_bytes(generated_image + b'# local edit\n')
        before = snapshot()
        failed = run('init', 'service', 'native-api', '--language', 'rust', '-D', 'Native API', success=False)
        assert 'Dockerfile was modified' in failed.stdout + failed.stderr
        assert snapshot() == before, 'Rejected custom image replaced application files'
        managed_image.write_bytes(generated_image)
        run('init', 'service', 'native-api', '--language', 'rust', '-D', 'Native API')
        assert not managed_image.exists(), 'Generated image should move to its language-specific sibling'
        managed_image.write_text(custom_dockerfile)

        run('init', 'worker', 'native-jobs', '--language', 'rust', '-t', 'redis', '--consumer-only', '-D', 'Native jobs')
        run('init', 'library', 'native-models', '--language', 'rust', '-D', 'Native models')
        run('init', 'module', 'native-cac', '--language', 'rust', '-m', 'cac-base')
        preserve_sources(original)
        assert all((app / name).is_file() for name in ['Dockerfile.rust', 'Dockerfile.typescript'])
        assert not (app / 'modules/Dockerfile').exists()
        dockerfile = (app / 'Dockerfile.rust').read_text()
        assert 'cargo build --locked --release --workspace --bins' in dockerfile
        assert '**/node_modules/' in (app / 'Dockerfile.rust.dockerignore').read_text()
        assert 'COPY modules/legacy-api/ modules/legacy-api/' not in dockerfile
        assert 'COPY . .' not in dockerfile
        manifest = tomllib.loads((app / '.forklaunch/manifest.toml').read_text())
        assert manifest['runtime'] == runtime and manifest['modules_path'] == 'modules'
        assert manifest['language'] == manifest['applicationLanguage'] == 'mixed'
        assert manifest['cargoWorkspace'] == 'Cargo.toml'
        assert manifest['sdkTargets'] == ['typescript', 'rust']
        projects = {project['name']: project for project in manifest['projects']}
        assert set(projects) >= {'legacy-api', 'native-api', 'native-jobs', 'native-models', 'native-cac'}
        for name, relative in [('native-api', 'modules/native-api'), ('native-jobs', 'modules/native-jobs'),
                               ('native-models', 'modules/core/native-models'), ('native-cac', 'modules/native-cac')]:
            assert projects[name]['metadata']['cratePath'] == relative
            assert (app / relative / 'Cargo.toml').is_file()
            assert (app / relative / 'lib.rs').is_file()
        assert projects['native-jobs']['metadata']['workerService'] is False
        worker_cargo = tomllib.loads((app / 'modules/native-jobs/Cargo.toml').read_text())
        assert {target['name'] for target in worker_cargo['bin']} == {'native-jobs'}
        config = compose()
        images = {'rustImage': rust_image()}
        rust_processes = images['rustImage']['processes']
        assert set(rust_processes) == {'native-api', 'native-jobs', 'native-cac'}
        assert {'modules/native-api/', 'modules/native-jobs/', 'modules/core/native-models/',
                'modules/native-cac/'} <= set(images['rustImage']['inputs'])
        assert 'modules/legacy-api/' not in images['rustImage']['inputs']
        assert 'modules/' not in images['rustImage']['inputs']
        assert len({config['services'][name]['image'] for name in rust_processes}) == 1
        builds = [config['services'][name]['build'] for name in rust_processes
                  if config['services'][name].get('build')]
        assert len(builds) == 1, 'One shared Rust image should have one Compose build recipe'
        assert builds[0]['context'] == str(app)
        assert Path(builds[0]['dockerfile']).as_posix().removeprefix('./') == 'Dockerfile.rust'
        for name in rust_processes:
            service = config['services'][name]
            assert service['command'] == ['/app/bin/' + name]
        assert 'native-jobs-server' not in config['services']
        assert (app / 'Dockerfile.typescript').read_text().startswith('# Generated by ForkLaunch (sha256:')
        assert Path(config['services']['legacy-api']['build']['dockerfile']).as_posix().removeprefix('./') == 'Dockerfile.typescript'

        source = app / 'modules/native-api/lib.rs'
        source.write_text(source.read_text() + '\npub const APPLICATION_OWNED: bool = true;\n')
        native_source = source.read_bytes()
        run('init', 'service', 'native-api', '--language', 'rust', '-D', 'Native API')
        assert source.read_bytes() == native_source
        preserve_sources(original)
        relocated = root / (runtime + '-relocated')
        app.rename(relocated)
        app = relocated
        assert not (root / runtime).exists()
        execute(['cargo', 'check', '--workspace', '--all-targets'])
        assert not (app / 'src/client-sdk').exists(), 'SDK compilation ignored the configured modules directory'
        execute(['cargo', 'build', '-p', 'native-api'])
        execute(['node', str(repo / 'cli/tests/fixtures/rust_mcp_service_startup.mjs'),
                 str(Path(env['CARGO_TARGET_DIR']) / 'debug/native-api')], timeout=120)
        preserve_sources(original)
        assert (app / 'modules/native-api/lib.rs').read_bytes() == native_source
        print(f'PASS {runtime} TypeScript-first native service/consumer/library/module attachment, relocation, authored-source preservation and HTTP/MCP startup', flush=True)

    # Exercise the Node fixture's one shared Rust image only after both native
    # runtime cases passed, so Docker compilation cannot mask a Bun regression.
    app = root / 'node-relocated'
    execute(['bash', str(repo / 'cli/tests/rust_container_runtime.sh')], extra_env={
        'FORKLAUNCH_RUST_CONTAINER_APP': str(app),
        'FORKLAUNCH_RUST_CONTAINER_SERVICE': 'native-api',
        'FORKLAUNCH_RUST_CONTAINER_DOCKERFILE': 'Dockerfile.rust',
    }, timeout=2400)
    print('PASS TypeScript-first Node/Bun attachment and relocated Node shared Docker image', flush=True)
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print(f'Mixed attachment fixture evidence: {root}', flush=True)
    else:
        shutil.rmtree(root)

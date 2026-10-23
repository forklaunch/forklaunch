#!/usr/bin/env python3
"""Exercise all raw built-in native module variants through their actual lifecycle."""
import hashlib
import json
import os
import re
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import tomllib

repo = Path(__file__).resolve().parents[3]
cli = Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch')).resolve()
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'CARGO_INCREMENTAL': '0',
       'RUSTUP_TOOLCHAIN': 'stable'}
env.setdefault('CARGO_TARGET_DIR', str(repo / 'cli/target'))
if not env.get('FORKLAUNCH_WORKER_RUNTIME_LOCKED'):
    sys.exit(subprocess.run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'),
        '-c', shlex.join([sys.executable, str(Path(__file__).resolve())])], env=env).returncode)
root = Path(tempfile.mkdtemp(prefix='forklaunch-module-lifecycle-')).resolve()
app = root / 'app'

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
    # The one service that builds Dockerfile.rust names the shared image tag.
    images = {service['image'] for service in services.values()
              if Path((service.get('build') or {}).get('dockerfile', '')).name == 'Dockerfile.rust'}
    assert len(images) == 1, images
    return {'inputs': inputs, 'binaries': binaries, 'processes': processes, 'image': images.pop()}

variants = ['iam-base', 'cac-base', 'ecommerce-stripe', 'billing-base', 'billing-stripe']
print('CLI SHA256:', hashlib.sha256(cli.read_bytes()).hexdigest(), flush=True)


def command(*args, cwd=None):
    print('Running:', shlex.join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd or app, env=env, check=True, timeout=1800)


def run(*args, path=None):
    command(cli, *args, '--path', path or app, cwd=root)


def check_sdks(names):
    command('cargo', 'check', '--workspace', '--all-targets')
    command('pnpm', 'run', 'sdk:typecheck')
    command('pnpm', '--filter', '@module-matrix/client-sdk', 'build')
    # External callers depend on the local SDK crates, never the servers.
    consumer = root / 'rust-consumer'
    (consumer / 'src').mkdir(parents=True, exist_ok=True)
    root_cargo = tomllib.loads((app / 'Cargo.toml').read_text())
    (consumer / 'Cargo.toml').write_text(f'''[package]
name = "module-sdk-consumer"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
''' + ''.join(f'{name.replace("-", "_")} = {{package="{name}-sdk", path={json.dumps(str(app / "src" / name / "sdk/rust"))}}}\n'
              for name in names))
    (consumer / 'src/lib.rs').write_text(''.join(
        f'pub fn client_{name.replace("-", "_")}(_: {name.replace("-", "_")}::Client) {{}}\n'
        for name in names))
    command('cargo', 'check', '--manifest-path', consumer / 'Cargo.toml')
    sdk_source = (app / 'src/client-sdk/clientSdk.ts').read_text()
    package = json.loads((app / 'src/client-sdk/package.json').read_text())
    for name in names:
        directory = app / 'src' / name
        assert project(name)['metadata']['sdkLayout'] == 'project-local-v1'
        assert set(['src/' + name, 'src/' + name + '/.forklaunch/contracts',
                    'src/' + name + '/sdk/rust']) <= set(root_cargo['workspace']['members'])
        assert (directory / 'sdk/typescript/index.ts').read_text().strip() == "export type * from './sdk.js';"
        assert not (directory / 'sdk/typescript/runtime.ts').exists()
        assert not (app / 'src/client-sdk/typescript' / name).exists()
        assert not (app / 'src/client-sdk/rust' / (name + '.rs')).exists()
        assert re.search(rf'import type\s*\{{[^}}]+\}}\s*from\s*[\'\"]@module-matrix/{re.escape(name)}[\'\"]', sdk_source)
        assert package['devDependencies'][f'@module-matrix/{name}'] == 'workspace:*'
        assert f'@module-matrix/{name}' not in package.get('dependencies', {})
        cargo = tomllib.loads((directory / 'sdk/rust/Cargo.toml').read_text())
        assert name not in cargo['dependencies']
        assert cargo['build-dependencies']['contracts']['path'] == '../../.forklaunch/contracts'
    assert 'ForkLaunchGeneratedSdk' not in sdk_source


def manifest():
    return tomllib.loads((app / '.forklaunch/manifest.toml').read_text())


def project(name):
    return next(p for p in manifest()['projects'] if p['name'] == name)


def artifacts(name):
    metadata = project(name)['metadata']
    return [app / metadata[field] for field in ['contractArtifactPath', 'openapiPath']]


def sdk_paths(name):
    return [app / 'src' / name / 'sdk/rust', app / 'src' / name / 'sdk/typescript']



def build_provenance_verifier():
    consumer = root / 'provenance-verifier'
    consumer.mkdir()
    service = app / 'src' / variants[0]
    manifest = tomllib.loads((service / 'Cargo.toml').read_text())
    # Verify with the same framework release the generated module builds with.
    spec = manifest['build-dependencies']['forklaunch-core']
    framework = (f'path={json.dumps(str((service / spec["path"]).resolve()))}' if 'path' in spec
                 else f'version={json.dumps(spec["version"])}')
    (consumer / 'Cargo.toml').write_text(f"""[package]
name = "module-provenance-verifier"
version = "0.0.0"
edition = "2024"
[workspace]
[[bin]]
name = "module-provenance-verifier"
path = "main.rs"
[dependencies]
forklaunch-core = {{{framework}, default-features=false, features=["build"]}}
serde_json = "1"
""")
    shutil.copyfile(repo / 'cli/tests/fixtures/rust_module_provenance.rs', consumer / 'main.rs')
    command('cargo', 'build', '--manifest-path', consumer / 'Cargo.toml')
    return Path(env['CARGO_TARGET_DIR']) / 'debug/module-provenance-verifier'


def save_compiler_artifacts(label):
    """Keep exact evidence outside the app so a failed comparison is reviewable."""
    snapshot = {file: file.read_bytes() for name in variants for file in artifacts(name)}
    for file, contents in snapshot.items():
        destination = root / 'artifact-snapshots' / label / file.relative_to(app)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(contents)
    return snapshot


def compare_compiler_modes(checked, runtime):
    """Distinguish a compiler serialization change from a wire-contract change."""
    report = {
        str(file.relative_to(app)): {
            'workspace_check_sha256': hashlib.sha256(contents).hexdigest(),
            'runtime_build_sha256': hashlib.sha256(runtime[file]).hexdigest(),
            'identical_bytes': contents == runtime[file],
            'identical_json': json.loads(contents) == json.loads(runtime[file]),
        }
        for file, contents in checked.items()
    }
    (root / 'artifact-snapshots/compiler-mode-comparison.json').write_text(
        json.dumps(report, indent=2) + '\n')
    changed = [file for file, result in report.items() if not result['identical_json']]
    assert not changed, f'Runtime compilation changed declared wire contracts: {changed}'
    reordered = [file for file, result in report.items() if not result['identical_bytes']]
    print(f'PASS compiler-mode semantic stability for {len(report)} artifacts; '
          f'{len(reordered)} serialization differences recorded separately', flush=True)


def verify_export_provenance(names, compiler_artifacts, verifier):
    # Export must preserve the compiler's exact bytes: equal JSON with different
    # formatting is still a change to a recorded asset digest.
    for file, contents in compiler_artifacts.items():
        assert file.read_bytes() == contents, f'Export rewrote compiler artifact: {file}'

    def snapshot():
        result = {}
        for directory, folders, files in os.walk(app):
            folders[:] = [name for name in folders if name not in
                          ['node_modules', 'vendor', 'target', '.git']]
            for name in files:
                file = Path(directory) / name
                if file.is_file():
                    result[str(file.relative_to(app))] = hashlib.sha256(file.read_bytes()).hexdigest()
        return result

    before = snapshot()
    arguments = []
    for name in names:
        arguments.extend([app / 'src' / name, artifacts(name)[0].with_name('provenance.json')])
    # This executable was compiled before export. No Cargo/build hook can repair
    # stale output between export and the actual framework verification call.
    command(verifier, *arguments)
    assert before == snapshot(), 'Read-only provenance verification changed the application'
    print('PASS unchanged compiler artifact bytes and declared HTTP provenance for all8; '
          'independent target/features and runtime policy remain outside this proof', flush=True)


def check_images(names):
    """Each runnable module shares the same scoped image and exec-form process."""
    rust = rust_image()
    dockerfile = (app / 'Dockerfile.rust').read_text()
    header, body = dockerfile.split('\n', 1)
    assert hashlib.sha256(body.encode()).hexdigest() in header, 'Dockerfile.rust must carry its generation fingerprint'
    assert 'USER 65532:65532' in dockerfile
    assert 'cargo build --locked --release --workspace --bins' in dockerfile
    assert 'COPY . .' not in dockerfile
    assert 'COPY --from=builder /app/target/release /app/target/release' not in dockerfile
    assert set(rust['binaries']) == set(names), rust
    expected = {name: [f'/app/bin/{name}'] for name in names}
    for name in names:
        if project(name)['variant'] == 'ecommerce-stripe':
            expected[name + '-inventory-worker'] = [f'/app/bin/{name}', '--worker']
        assert f'COPY --from=builder /app/target/release/{name} /app/bin/{name}\n' in dockerfile
        assert f'src/{name}/' in rust['inputs']
    assert set(rust['processes']) == set(expected), rust['processes']
    # Compose normalization checks the emitted document, without building or
    # starting containers (the dedicated ecommerce gate runs both processes).
    compose = json.loads(subprocess.check_output(['docker', 'compose', '-f',
        str(app / 'docker-compose.yaml'), 'config', '--no-interpolate', '--format', 'json'],
        cwd=app, env=env, text=True))
    builders = []
    for name, command in expected.items():
        service = compose['services'][name]
        assert service['command'] == command, (name, service)
        assert service['image'] == rust['image'], (name, service)
        if service.get('build'):
            builders.append(service['build'])
    assert len(builders) == 1, builders
    assert Path(builders[0]['context']).resolve() == app.resolve()
    assert Path(builders[0]['dockerfile']).as_posix().removeprefix('./') == 'Dockerfile.rust'
    assert (app / 'Dockerfile.rust.dockerignore').is_file()


try:
    run('init', 'application', 'module-matrix', '--database', 'postgresql', '--validator', 'zod',
        '--formatter', 'prettier', '--linter', 'oxlint', '--http-framework', 'express',
        '--runtime', 'node', '--test-framework', 'vitest', '-D', 'Module lifecycle matrix',
        '-A', 'ForkLaunch', '-L', 'MIT')
    run('init', 'library', 'shared-client', '--language', 'typescript', '-D', 'Shared client')
    for index, variant in enumerate(variants):
        run('init', 'module', variant, '--language', 'rust', '-m', variant)
        check_images(variants[:index + 1])
    app.rename(root / 'relocated')
    app = root / 'relocated'
    assert not (root / 'app').exists()
    client_path = app / project('shared-client').get('metadata', {}).get('staticPath', 'src/shared-client')
    assert (client_path / 'package.json').is_file()
    client_files = {p.relative_to(client_path): p.read_bytes() for p in client_path.rglob('*') if p.is_file()}

    for variant in variants:
        source = app / 'src' / variant
        with (source / 'lib.rs').open('a') as file:
            file.write('\npub const USER_LIFECYCLE_MARKER: &str = "preserved";\n')
        with (source / 'Cargo.toml').open('a') as file:
            file.write('\n[package.metadata.user]\nmarker = "preserved"\n')
        run('sync', 'service', variant)
        check_images(variants)
        run('init', 'module', variant, '--language', 'rust', '-m', variant)
        assert 'USER_LIFECYCLE_MARKER' in (source / 'lib.rs').read_text()
        assert project(variant)['variant'] == variant
        check_images(variants)

    verifier = build_provenance_verifier()
    command('cargo', 'check', '--workspace')
    checked_artifacts = save_compiler_artifacts('workspace-check')
    run('openapi', 'export')
    # Export compiles each runtime binary. Cargo may use a different dependency
    # feature union than `check --workspace`; preserve the semantic contract and
    # verify the exact output of that real compilation before testing reuse.
    compiler_artifacts = save_compiler_artifacts('runtime-build')
    compare_compiler_modes(checked_artifacts, compiler_artifacts)
    verify_export_provenance(variants, compiler_artifacts, verifier)
    run('openapi', 'export')
    save_compiler_artifacts('unchanged-export')
    verify_export_provenance(variants, compiler_artifacts, verifier)
    for variant in variants:
        document = json.loads(artifacts(variant)[1].read_text())
        for group in document.get('components', {}).values():
            assert all(re.fullmatch(r'[a-zA-Z0-9._-]+', name) for name in group), variant
    run('sdk', 'generate', '--target', 'all', '--from', 'openapi')
    command('pnpm', 'install', '--ignore-scripts')
    check_sdks(variants)
    run('sdk', 'generate', '--target', 'all', '--from', 'contract')
    current_names = variants.copy()
    for index, variant in enumerate(variants):
        name = variant + '-renamed'
        old_artifacts = artifacts(variant)
        run('change', 'service', '-N', name, '--confirm', path=app / 'src' / variant)
        current_names[index] = name
        check_images(current_names)
        source = app / 'src' / name
        assert not (app / 'src' / variant).exists()
        assert project(name)['variant'] == variant
        assert 'USER_LIFECYCLE_MARKER' in (source / 'lib.rs').read_text()
        assert tomllib.loads((source / 'Cargo.toml').read_text())['package']['metadata']['user']['marker'] == 'preserved'
        run('sync', 'service', name)
        assert all(path.exists() for path in sdk_paths(name)), name
        assert all(path.is_file() for path in artifacts(name)), name
        assert all(not path.exists() for path in old_artifacts), variant
        assert all(not path.exists() for path in sdk_paths(variant)), variant
        check_images(current_names)
        print(f'PASS {variant}: init, relocation, sync, idempotency, rename, source and variant preservation', flush=True)

    run('sdk', 'generate', '--target', 'all', '--from', 'contract', '--check')
    # Renaming changed the SDK package manifests; CI's pnpm defaults to a frozen
    # lockfile, so refresh it the way a developer does after a rename.
    command('pnpm', 'install', '--ignore-scripts', '--no-frozen-lockfile')
    check_sdks([name + '-renamed' for name in variants])
    for variant in variants:
        name = variant + '-renamed'
        owned = artifacts(name) + sdk_paths(name)
        run('delete', 'service', name, '--continue')
        assert not (app / 'src' / name).exists()
        assert all(not path.exists() for path in owned), name
        assert all(p['name'] != name for p in manifest()['projects'])
        assert all(name not in peers for peers in manifest()['project_peer_topology'].values())
        print(f'PASS {variant}: delete and removal of contracts, SDKs and registrations', flush=True)
    assert manifest()['language'] == 'mixed'
    assert client_files == {p.relative_to(client_path): p.read_bytes() for p in client_path.rglob('*') if p.is_file()}
    command('cargo', 'metadata', '--no-deps', '--format-version', '1')

    # Model a previously generated module using the complete raw manifest and
    # actual module sources. Reinitialization must not silently move its SDKs.
    legacy = 'legacy-cac'
    run('init', 'module', legacy, '--language', 'rust', '-m', 'cac-base')
    directory = app / 'src' / legacy
    cargo_path = directory / 'Cargo.toml'
    cargo_source = re.sub(r'^contracts\s*=.*\n', '', cargo_path.read_text(), flags=re.M)
    cargo_source = re.sub(r'^\[(?:build-dependencies|dependencies)\.contracts\]\n.*?(?=^\[|\Z)', '', cargo_source, flags=re.M | re.S)
    cargo_path.write_text(cargo_source)
    (directory / 'api/contracts/mod.rs').write_text('pub mod registry;\npub mod extensions;\n')
    old_build = (repo / 'cli/src/templates/rust/project/module-sdk-build.rs').read_text().replace('{{{name}}}', legacy)
    (directory / 'build.rs').write_text(old_build)
    shutil.rmtree(directory / '.forklaunch/contracts')
    shutil.rmtree(directory / 'sdk')
    workspace = app / 'Cargo.toml'
    workspace_source = workspace.read_text()
    members = [member for member in tomllib.loads(workspace_source)['workspace']['members']
               if member not in ['src/legacy-cac/.forklaunch/contracts', 'src/legacy-cac/sdk/rust']]
    workspace.write_text(re.sub(r'^members\s*=\s*\[.*?\]', 'members = ' + json.dumps(members), workspace_source, flags=re.M | re.S))
    manifest_path = app / '.forklaunch/manifest.toml'
    source = manifest_path.read_text()
    begin = source.rfind('[[projects]]', 0, source.index('name = "legacy-cac"'))
    end = source.find('[[projects]]', begin + 1)
    end = len(source) if end == -1 else end
    section = source[begin:end]
    section = re.sub(r'^sdkLayout\s*=.*\n', '', section, flags=re.M)
    section = re.sub(r'^contractArtifactPath\s*=.*$', 'contractArtifactPath = ".forklaunch/contracts/legacy-cac/contract.json"', section, flags=re.M)
    section = re.sub(r'^openapiPath\s*=.*$', 'openapiPath = ".forklaunch/openapi/legacy-cac/openapi.json"', section, flags=re.M)
    manifest_path.write_text(source[:begin] + section + source[end:])
    preserved = {file: file.read_bytes() for file in [cargo_path, directory / 'build.rs', directory / 'api/contracts/registry.rs']}
    run('init', 'module', legacy, '--language', 'rust', '-m', 'cac-base')
    assert all(file.read_bytes() == contents for file, contents in preserved.items())
    assert not (directory / 'sdk/rust').exists()
    assert 'sdkLayout' not in project(legacy)['metadata']
    command('cargo', 'check', '-p', legacy)
    assert (app / 'src/client-sdk/typescript/legacy-cac/index.ts').is_file()
    assert (app / 'src/client-sdk/rust/legacy-cac.rs').is_file()
    run('openapi', 'export')
    run('sdk', 'generate', '--target', 'all', '--from', 'contract')
    run('sdk', 'generate', '--target', 'all', '--from', 'contract', '--check')
    assert not (directory / 'sdk/rust').exists()
    sys.path.insert(0, str(repo / 'cli/tests/fixtures'))
    from rust_builtin_runtime_support import rust_consumer
    consumer_manifest = rust_consumer(app, root / 'legacy-rust-consumer', legacy)
    command('cargo', 'check', '--manifest-path', consumer_manifest)
    print('PASS existing aggregate module preserves source/paths through reinit, compilation, export, SDK generation/check and external consumption', flush=True)
    print('PASS all eight module variants and preservation of the TypeScript library', flush=True)
finally:
    if env.get('FORKLAUNCH_KEEP_FIXTURE'):
        print('Module lifecycle fixture evidence:', root, flush=True)
    else:
        shutil.rmtree(root)

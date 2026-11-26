#!/usr/bin/env python3
"""Build a raw CLI app against exact packaged or published framework versions."""
import argparse
from contextlib import nullcontext
import hashlib
import io
import json
import os
import re
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

repo = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(repo / 'scripts'))
from rust_registry import checked_archive, manifest_text, registry, safe_members

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--release', type=Path, required=True)
parser.add_argument('--docker', action='store_true', help='Build and run the generated shared Docker image (public registry only)')
parser.add_argument('--default-source', action='store_true', help='Verify the published default without a version flag')
parser.add_argument('--staging', action='store_true', help='Serve verified package archives through an isolated sparse registry')
parser.add_argument('--mixed', action='store_true', help='Build a raw TypeScript Better Auth module and Rust SDK aggregate using the published default')
parser.add_argument('--receipt', type=Path, help='Write successful validation evidence separately from the release directory')
parser.add_argument('--blueprint-release', type=Path, help='Serve prepared blueprint crates through an isolated sparse registry (framework stays on crates.io)')
args = parser.parse_args()
if args.docker and args.staging:
    parser.error('--docker requires public registry dependencies')
if args.blueprint_release and args.docker:
    parser.error('--blueprint-release serves a loopback registry; it cannot combine with --docker')
if args.mixed and (args.staging or not args.default_source):
    parser.error('--mixed requires --default-source and public registry dependencies')
report = json.loads((args.release / 'release.json').read_text())
assert report['scope'] == 'framework'
versions = {p['name']: p['version'] for p in report['packages']}
blueprint_report = json.loads((args.blueprint_release / 'release.json').read_text()) if args.blueprint_release else None
assert blueprint_report is None or blueprint_report['scope'] == 'blueprint'
# Without a staged blueprint release, generated apps pin the published blueprint
# crates at the CLI's BLUEPRINT_VERSION.
published_blueprint = re.search(r'BLUEPRINT_VERSION: &str = "([^"]+)"', (repo / 'cli/src/core/cargo_toml/cargo_toml_constants.rs').read_text()).group(1)
blueprint_versions = {p['name']: p['version'] for p in blueprint_report['packages']} if blueprint_report else {
    name: published_blueprint for name in ['forklaunch-worker-interface', 'forklaunch-worker-database', 'forklaunch-worker-redis', 'forklaunch-worker-kafka']}
cli = Path(os.environ['FORKLAUNCH_CLI']).resolve()
env = {**os.environ, 'RUSTUP_TOOLCHAIN': 'stable', 'CARGO_INCREMENTAL': '0'}
env.pop('FORKLAUNCH_SKIP_FORMAT', None)
env.pop('FORKLAUNCH_CONTRACT_COMPILER', None)
env.setdefault('CARGO_TARGET_DIR', str(repo / 'cli/target'))
if not env.get('FORKLAUNCH_WORKER_RUNTIME_LOCKED'):
    sys.exit(subprocess.run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'), '-c', shlex.join([sys.executable, str(Path(__file__).resolve()), *sys.argv[1:]])], env=env).returncode)
root = Path(tempfile.mkdtemp(prefix='forklaunch-registry-scaffold-'))
app = root / 'app'

def run(*command, **kwargs):
    print('Running:', shlex.join(map(str, command)), flush=True)
    return subprocess.run(list(map(str, command)), cwd=app if app.exists() else repo, env=env, check=True, **kwargs)

def command(*arguments):
    run(cli, *arguments, '--path', app)

def dependency_manifests():
    """Snapshot local install inputs without traversing caches or build outputs."""
    manifests = {}
    fields = ['name', 'version', 'packageManager', 'workspaces', 'dependencies',
              'devDependencies', 'peerDependencies', 'peerDependenciesMeta',
              'optionalDependencies', 'bundledDependencies', 'bundleDependencies',
              'overrides', 'resolutions', 'pnpm']
    for directory, children, files in os.walk(app, followlinks=False):
        children[:] = sorted(child for child in children
                             if child not in {'node_modules', 'target', '.git', 'dist', 'lib', 'production', 'vendor'}
                             and not (Path(directory) / child).is_symlink())
        if 'package.json' in files:
            path = Path(directory) / 'package.json'
            package = json.loads(path.read_text())
            manifests[str(path.relative_to(app))] = {field: package[field] for field in fields if field in package}
    return manifests

def workflow_steps(workflow):
    """Read the generated CI's run steps; the generator emits one shape only."""
    steps, job, name, directory = [], None, None, '.'
    for line in workflow.splitlines():
        if line.startswith('  ') and not line.startswith('   ') and line.rstrip().endswith(':'):
            job = line.strip()[:-1]
        elif line.startswith('      - name: '):
            name, directory = line.removeprefix('      - name: '), '.'
        elif line.startswith('      - run: '):
            steps.append((job, line.removeprefix('      - run: '), '.', line.removeprefix('      - run: ')))
        elif line.startswith('        working-directory: '):
            directory = line.removeprefix('        working-directory: ')
        elif line.startswith('        run: '):
            steps.append((job, name, directory, line.removeprefix('        run: ')))
    return steps

def run_generated_ci():
    """Execute every generated CI command in a committed checkout of the app.
    Toolchain provisioning is host-specific and recorded as skipped."""
    # The documented first step: install once and commit the lockfile.
    if (app / 'package.json').is_file() and not (app / 'pnpm-lock.yaml').is_file():
        run('pnpm', 'install', timeout=900)
    # A modules directory that is its own workspace root keeps its own lockfile.
    if (app / 'src/pnpm-workspace.yaml').is_file() and (app / 'pnpm-workspace.yaml').is_file():
        subprocess.run(['pnpm', 'install'], cwd=app / 'src', env=env, check=True, timeout=900)
    for git in [('init', '-q'), ('add', '-A'), ('-c', 'user.name=ForkLaunch CI', '-c', 'user.email=ci@forklaunch.invalid', 'commit', '-qm', 'Generated application')]:
        run('git', *git)
    results = []
    for job, name, directory, script in workflow_steps((app / '.github/workflows/ci.yml').read_text()):
        if script.startswith(('rustup ', 'corepack ', 'npm install -g')) or (job == 'docker' and not args.docker):
            results.append({'job': job, 'step': name, 'command': script, 'status': 'skipped'})
            continue
        print(f'Running generated CI [{job}] {name}: {script}', flush=True)
        result = subprocess.run(['bash', '-eo', 'pipefail', '-c', script], cwd=app / directory, env=env, timeout=3600)
        results.append({'job': job, 'step': name, 'directory': directory, 'command': script,
                        'status': 'passed' if result.returncode == 0 else f'failed:{result.returncode}'})
    return results

def assert_dependency_manifests(expected):
    actual = dependency_manifests()
    changed = sorted(path for path in expected.keys() | actual.keys() if expected.get(path) != actual.get(path))
    assert not changed, f'Compilation changed package installation inputs after pnpm install: {changed}'

try:
    source_options = [] if args.default_source else ['--framework-version', versions['forklaunch-core']]
    command('init', 'application', 'registry-proof', *source_options, '--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint', '--http-framework', 'express', '--runtime', 'node', '--test-framework', 'vitest', '-D', 'Registry proof', '-A', 'ForkLaunch', '-L', 'MIT')
    command('init', 'service', 'notes', '--language', 'rust', '--mappers', '--infrastructure', 'redis', 's3', '-D', 'Registry service')
    command('init', 'worker', 'jobs', '--language', 'rust', '--type', 'database', '--database', 'postgresql', '--runtime', 'apalis', '-D', 'Registry worker')
    if args.mixed:
        command('init', 'module', '--language', 'typescript', '--module', 'iam-better-auth', '--database', 'postgresql')
        command('init', 'library', 'registry-clients', '--language', 'rust', '--sdk-services', 'notes', 'jobs')
    assert not (app / 'vendor/framework').exists()
    assert not (app / '.forklaunch/framework-rs').exists()
    # Blueprint crates are registry dependencies like the framework; no copy.
    assert not (app / '.forklaunch/blueprint-rs').exists()
    blueprint_pins = sorted({f"{dependency.get('package', alias)}{dependency['version']}"
        for path in app.rglob('Cargo.toml') if 'target' not in path.parts and 'node_modules' not in path.parts
        for group in ('dependencies', 'build-dependencies')
        for alias, dependency in tomllib.loads(path.read_text()).get(group, {}).items()
        if isinstance(dependency, dict) and dependency.get('package', alias) in blueprint_versions})
    assert blueprint_pins, 'The Apalis worker depends on published blueprint crates'
    assert all('path' not in str(pin) for pin in blueprint_pins)
    workflow = (app / '.github/workflows/ci.yml').read_text()
    for check in ['cargo fmt -- --check', 'cargo clippy --workspace --all-targets --locked', 'cargo test --workspace --locked', 'git status --porcelain']:
        assert check in workflow, check
    assert '**/.forklaunch/compliance.json' in (app / '.gitignore').read_text().splitlines()
    # Applications use upstream serde_json: no root patch and no vendored copy.
    assert not (app / 'vendor/serde_json').exists()
    assert 'patch' not in tomllib.loads((app / 'Cargo.toml').read_text())
    # One loopback index serves every unpublished release the scaffold needs.
    staged = ([(args.release, report)] if args.staging else []) + ([(args.blueprint_release, blueprint_report)] if blueprint_report else [])
    with (registry(*staged[0], *staged[1:]) if staged else nullcontext(None)) as shared_index:
        index = shared_index if args.staging else None
        blueprint_index = shared_index if blueprint_report else None
        if blueprint_index:
            (app / '.cargo').mkdir(exist_ok=True)
            (app / '.cargo/config.toml').write_bytes(manifest_text({'registries': {'forklaunch-staging': {'index': blueprint_index}}}))
            for path in app.rglob('Cargo.toml'):
                if 'target' in path.parts or 'node_modules' in path.parts:
                    continue
                value = tomllib.loads(path.read_text())
                changed = False
                for group in ['dependencies', 'build-dependencies', 'dev-dependencies']:
                    for alias, dependency in value.get(group, {}).items():
                        name = dependency.get('package', alias) if isinstance(dependency, dict) else alias
                        if name in blueprint_versions:
                            assert isinstance(dependency, dict) and 'path' not in dependency, dependency
                            assert dependency['version'] == '=' + blueprint_versions[name], dependency
                            dependency['registry'] = 'forklaunch-staging'
                            changed = True
                if changed:
                    path.write_bytes(manifest_text(value))
        if index:
            (app / '.cargo').mkdir(exist_ok=True)
            (app / '.cargo/config.toml').write_bytes(manifest_text({'registries': {'forklaunch-staging': {'index': index}}}))
            # Local blueprint adapters and contract hosts must resolve the same
            # published framework graph as the application packages.
            for path in app.rglob('Cargo.toml'):
                value = tomllib.loads(path.read_text())
                changed = False
                for group in ['dependencies', 'build-dependencies', 'dev-dependencies']:
                    for alias, dependency in value.get(group, {}).items():
                        name = dependency.get('package', alias) if isinstance(dependency, dict) else alias
                        if name in versions:
                            assert isinstance(dependency, dict) and 'path' not in dependency, dependency
                            assert dependency['version'] == '=' + versions[name]
                            dependency['registry'] = 'forklaunch-staging'
                            changed = True
                if changed:
                    path.write_bytes(manifest_text(value))
            # Published blueprint crates depend on crates.io framework crates;
            # point those at the staged release so the graph has one copy.
            workspace = tomllib.loads((app / 'Cargo.toml').read_text())
            workspace.setdefault('patch', {})['crates-io'] = {
                name: {'version': '=' + version, 'registry': 'forklaunch-staging'} for name, version in versions.items()}
            (app / 'Cargo.toml').write_bytes(manifest_text(workspace))
        mixed_checks = None
        # Every generated artifact lives in its project; .forklaunch/ holds only the manifest.
        assert sorted(str(path.relative_to(app)) for path in (app / '.forklaunch').rglob('*')) == ['.forklaunch/manifest.toml']
        if args.mixed:
            # The manifest is the only committed state; contract builds are a
            # manifest flag and the CLI compiles them in-process.
            state = sorted(str(path.relative_to(app)) for path in (app / '.forklaunch').rglob('*') if path.is_file())
            assert state == ['.forklaunch/manifest.toml'], state
            manifest = tomllib.loads((app / '.forklaunch/manifest.toml').read_text())
            iam = next(project for project in manifest['projects'] if project['name'] == 'iam')
            assert iam['metadata']['contractBuild'] is True and iam['metadata']['sdkLayout'] == 'project-local-v1'
            workspace = tomllib.loads((app / 'Cargo.toml').read_text())
            assert not any('contract-compiler' in member for member in workspace['workspace']['members'])
            artifacts = [app / 'src/iam/sdk' / name for name in ['contract.json', 'openapi.json', 'rust/lib.rs']]
            assert not artifacts[0].exists() and not artifacts[1].exists(), 'Raw init must wait for compilation to emit the IAM contract'
            package = json.loads((app / 'src/iam/package.json').read_text())
            assert package['scripts']['build'] == 'tsc -b'
            assert package['scripts']['sdk:build'] == 'forklaunch sdk build --service iam'
            # Installing first must discover the encrypted event package too.
            # A preparatory Cargo build would conceal a missing raw manifest.
            assert not (app / 'Cargo.lock').exists(), 'Mixed first-install proof must precede Cargo preparation'
            event_manifest_path = app / 'src/jobs/events/typescript/package.json'
            assert event_manifest_path.is_file(), 'Raw worker init must declare its event package before installation'
            event_manifest = json.loads(event_manifest_path.read_text())
            assert event_manifest['name'] == '@forklaunch-generated/jobs-events'
            assert {'ajv', 'ajv-formats', '@types/node'} <= event_manifest['dependencies'].keys()
            assert 'typescript' in event_manifest['devDependencies']
            assert event_manifest_path.with_name('tsconfig.json').is_file()
            install_inputs = dependency_manifests()
            # Run the documented application commands without injecting the
            # compiler executable. The root build also creates the shared lock.
            run('pnpm', 'install', timeout=600)
            assert_dependency_manifests(install_inputs)
            run('pnpm', 'run', 'build', timeout=1800)
            command('sdk', 'build', '--service', 'iam', '--skip-compile')
            assert_dependency_manifests(install_inputs)
            assert json.loads(event_manifest_path.read_text()) == event_manifest
            assert all(path.is_file() for path in artifacts)
            contract = json.loads(artifacts[0].read_text())
            openapi = json.loads(artifacts[1].read_text())
            # Native exports are documents; TypeScript exports a version map.
            documents = [openapi] if 'openapi' in openapi else list(openapi.values())
            assert documents and all(document.get('openapi', '').startswith('3.') and isinstance(document.get('paths'), dict) for document in documents)
            assert contract['formatVersion'] == 'forklaunch.contract.v1' and contract['routes']
            assert any(document['paths'] for document in documents)
            rust_sdk = artifacts[2].parent
            rust_source = '\n'.join(path.read_text() for path in rust_sdk.rglob('*.rs'))
            assert 'pub struct Client' in rust_source, 'The contract build must emit the typed IAM Rust client'
            # Rebuilding the SDK is idempotent.
            digests = {path: path.read_bytes() for path in artifacts}
            command('sdk', 'build', '--service', 'iam', '--skip-compile')
            assert all(path.read_bytes() == content for path, content in digests.items())
            assert_dependency_manifests(install_inputs)
            mixed_checks = {'typescriptModule': 'iam-better-auth', 'rustAggregate': 'registry-clients',
                            'normalPnpmInstallAndBuild': True, 'compilerOverride': False,
                            'workerEventPackagePresentBeforeInstall': True, 'packageDependenciesStableAfterBuild': True,
                            'rawDependencyManifestSha256': hashlib.sha256(json.dumps(install_inputs, sort_keys=True).encode()).hexdigest(),
                            'manifestOnlyState': True, 'sdkRebuildIdempotent': True,
                            'iamArtifactSha256': {str(path.relative_to(app)): hashlib.sha256(path.read_bytes()).hexdigest() for path in artifacts}}
        run('cargo', 'check', '--workspace', '--all-targets')
        run('cargo', 'test', '-p', 'notes', '--test', 'http')
        run('cargo', 'build', '-p', 'notes', '-p', 'jobs', '--bins')
        metadata = json.loads(run('cargo', 'metadata', '--locked', '--format-version', '1', capture_output=True, text=True).stdout)
        installed = []
        for package in metadata['packages']:
            if package['name'] in versions:
                assert package['version'] == versions[package['name']]
                assert package['source'] and (index.removeprefix('sparse+') if index else 'crates.io-index') in package['source'], package
                installed.append(package['name'])
        assert {'forklaunch-core', 'forklaunch-axum', 'forklaunch-infrastructure-redis', 'forklaunch-infrastructure-s3', 'forklaunch-universal-sdk'} <= set(installed)
        for package in metadata['packages']:
            if package['name'] in blueprint_versions:
                assert package['version'] == blueprint_versions[package['name']]
                assert package['source'] and (blueprint_index.removeprefix('sparse+') if blueprint_index else 'crates.io-index') in package['source'], package
        if args.mixed:
            packages = {package['name']: package for package in metadata['packages']}
            assert {'iam-sdk', 'registry-clients'} <= packages.keys()
            assert 'forklaunch-contract-compiler' not in packages
            assert not (app / 'vendor/framework').exists() and not (app / '.forklaunch/framework-rs').exists()
        for artifact in ['sdk/typescript/openapi.json', 'sdk/typescript/contract.json', 'sdk/typescript/sdk.ts']:
            assert (app / 'src/notes' / artifact).is_file()
        command('sdk', 'generate', '--target', 'all', '--from', 'contract', '--check')
        if args.mixed:
            command('sdk', 'generate', '--target', 'rust', '--from', 'contract', '--service', 'iam', '--check')
            mixed_checks['iamSdkFreshness'] = True
        run('node', repo / 'cli/tests/fixtures/rust_mcp_service_startup.mjs', Path(env['CARGO_TARGET_DIR']) / 'debug/notes')
        run('node', repo / 'cli/tests/fixtures/rust_worker_service_policy.mjs', Path(env['CARGO_TARGET_DIR']) / 'debug/jobs')
        # Storage declarations are build outputs beside each crate's contract host.
        for crate in ['src/notes', 'src/jobs']:
            assert (app / crate / '.forklaunch/compliance.json').is_file(), crate
            assert not (app / crate / 'compliance.json').exists(), crate
        command('compliance', 'validate-artifacts')
        # Staged registries must still be serving while generated CI resolves crates.
        generated_ci = run_generated_ci()
    if args.docker:
        run('node', repo / 'cli/tests/fixtures/rust_registry_container.mjs', app)
    receipt = {'schemaVersion': 1, 'source': 'staging' if args.staging else 'crates.io', 'cliSha256': hashlib.sha256(cli.read_bytes()).hexdigest(), 'releaseSha256': hashlib.sha256((args.release / 'release.json').read_bytes()).hexdigest(), 'frameworkVersions': versions, 'rawScaffold': True, 'defaultDependencySource': args.default_source, 'frameworkSnapshot': False, 'parserPatch': False, 'infrastructureAdaptersCompiled': ['redis', 's3'], 'workerRuntime': 'apalis', 'workerProducerAuthorization': True, 'blueprintSource': 'staging' if blueprint_report else 'crates.io', 'blueprintPins': blueprint_pins, 'blueprintCopy': False, 'complianceArtifacts': ['src/notes/.forklaunch/compliance.json', 'src/jobs/.forklaunch/compliance.json'], 'generatedCi': generated_ci, 'sdkFreshness': True, 'httpMcpStartup': True, 'dockerHttpMcpStartup': args.docker, 'mixedBuild': mixed_checks}
    destination = args.receipt or args.release / ('generated-staging-app.json' if args.staging else 'generated-registry-app.json')
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(receipt, indent=2)+'\n')
    failed = [step for step in generated_ci if step['status'].startswith('failed')]
    assert not failed, f'Generated CI commands failed (receipt {destination}): {failed}'
    print('PASS raw CLI registry-mode generation, archive parser patch, Redis/S3 adapters, both SDKs and HTTP/MCP startup:', destination, flush=True)
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print('Fixture retained:', root, flush=True)
    else:
        shutil.rmtree(root)

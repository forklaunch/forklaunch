#!/usr/bin/env python3
"""Actual CLI lifecycle routing for registered Rust and TypeScript projects."""
import json
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import shutil
import tomllib

repo = Path(__file__).resolve().parents[3]
binary = Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch')).resolve()
output = repo / 'cli/tests/output/rust-mixed-lifecycle'
output.mkdir(parents=True, exist_ok=True)
root = Path(tempfile.mkdtemp(prefix='run.', dir=output)).resolve()
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
    return {'inputs': inputs, 'binaries': binaries, 'processes': processes}

account = 'parity-offline-lifecycle-' + root.name.removeprefix('run.').replace('_', '-')
account_path = Path.home() / '.forklaunch/accounts' / (account + '.toml')
assert not account_path.exists()
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'CARGO_INCREMENTAL': '0',
       'FORKLAUNCH_ACCOUNT': account}
shutil.copy2(binary, root / 'forklaunch')
binary = root / 'forklaunch'
cli_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
print('CLI SHA256:', cli_hash, flush=True)
mixed_fields = None


def run(*args, path=None):
    subprocess.run([str(binary), *args, '--path', str(path or app)], cwd=root, env=env, check=True, timeout=180)
    if mixed_fields is not None:
        actual = manifest()
        assert all(actual.get(key) == value for key, value in mixed_fields.items()), 'Lifecycle erased mixed application metadata'


def manifest():
    return tomllib.loads((app / '.forklaunch/manifest.toml').read_text())


run('init', 'application', 'mixed-lifecycle', '--modules-path', 'modules',
    '--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint',
    '--http-framework', 'express', '--runtime', 'node', '--test-framework', 'vitest',
    '-D', 'Mixed lifecycle', '-A', 'ForkLaunch', '-L', 'MIT')
for kind, name, extra in [('service', 'typescript-api', ['-d', 'postgresql']),
                          ('worker', 'typescript-jobs', ['-t', 'database', '-d', 'postgresql']),
                          ('library', 'typescript-models', [])]:
    run('init', kind, name, '--language', 'typescript', '-D', name, *extra)
# Project initialization owns mixed metadata and the root Cargo workspace.
# Preserve those actual generated values during every subsequent lifecycle call.
run('init', 'service', 'native-api', '--language', 'rust', '-D', 'Native API')
mixed_fields = {key: manifest()[key] for key in ['language', 'applicationLanguage', 'sdkTargets', 'cargoWorkspace', 'app_name']}
assert mixed_fields['language'] == mixed_fields['applicationLanguage'] == 'mixed'
assert set(mixed_fields['sdkTargets']) == {'rust', 'typescript'}
for kind, name, extra in [('worker', 'native-jobs', ['-t', 'redis', '--consumer-only']),
                          ('library', 'native-models', [])]:
    run('init', kind, name, '--language', 'rust', '-D', name, *extra)
native_paths = {project['name']: project['metadata']['cratePath'] for project in manifest()['projects'] if project.get('metadata', {}).get('language') == 'rust'}
image_inventory = rust_image()
assert {'native-api', 'native-jobs'} <= set(image_inventory['binaries'])
# Rust image may need the TypeScript projects' generated Rust client crates,
# but must not copy their application implementations or Node dependencies.
for name in ['typescript-api', 'typescript-jobs']:
    assert all(not item.startswith(f'modules/{name}/') or item.startswith(f'modules/{name}/sdk/rust/') for item in image_inventory['inputs'])
assert (app / 'Dockerfile.typescript').is_file()
assert not (app / 'modules/Dockerfile').exists()
assert 'COPY . .' not in (app / 'Dockerfile.rust').read_text()
assert '--locked --release --workspace --bins' in (app / 'Dockerfile.rust').read_text()
assert len(image_inventory['processes']) >= 2
print('PASS scoped shared Rust image and adjacent language Dockerfiles', flush=True)
for kind, name in [('service', 'typescript-api'), ('worker', 'typescript-jobs'), ('library', 'typescript-models')]:
    before = {project['name']: project for project in manifest()['projects']}
    source = {str(p.relative_to(app)): p.read_bytes() for p in (app / 'modules' / name).rglob('*') if p.is_file()}
    run('sync', kind, name)
    after = {project['name']: project for project in manifest()['projects']}
    assert name in after, f'Mixed sync removed the TypeScript {kind} registration'
    assert before == after, f'Mixed sync unexpectedly changed project registrations for {name}'
    assert source == {str(p.relative_to(app)): p.read_bytes() for p in (app / 'modules' / name).rglob('*') if p.is_file()}
    assert not (app / 'crates' / name).exists()
print('PASS mixed sync routes each TypeScript project without native cleanup', flush=True)
for kind, name in [('service', 'native-api'), ('worker', 'native-jobs'), ('library', 'native-models')]:
    run('sync', kind, name)
# Keep package-cache cleanup inside the fixture boundary during rename commands.
package = app / 'modules/package.json'
package_data = json.loads(package.read_text())
package_data['scripts']['clean:purge'] = "node -e \"require('fs').appendFileSync('.lifecycle-clean','clean;')\""
package.write_text(json.dumps(package_data))
for kind, name in [('service', 'typescript-api'), ('worker', 'typescript-jobs'), ('library', 'typescript-models')]:
    run('change', kind, '-N', name + '-renamed', '--confirm', path=app / 'modules' / name)
    assert (app / 'modules' / (name + '-renamed')).is_dir()
    assert name + '-renamed' in {project['name'] for project in manifest()['projects']}
    run('change', kind, '-N', name, '--confirm', path=app / 'modules' / (name + '-renamed'))
assert (app / 'modules/.lifecycle-clean').read_text().count('clean;') == 6
print('PASS mixed TypeScript service, worker and library rename roundtrips', flush=True)
# A router operation also resolves the owning project's language.
run('init', 'router', 'profiles', '--language', 'typescript', path=app / 'modules/typescript-api')
assert manifest()['language'] == 'mixed'
server = app / 'modules/typescript-api/server.ts'
original = server.read_text()
assert 'app.use(profilesRouter)' in original
server.write_text(original.replace('app.use(profilesRouter);', ''))
owned = app / 'modules/typescript-api'
before = {str(p.relative_to(app)): p.read_bytes() for p in owned.rglob('*') if p.is_file()}
manifest_before = (app / '.forklaunch/manifest.toml').read_bytes()
failed = subprocess.run([str(binary), 'delete', 'router', 'profiles', '--continue', '--path', str(owned)],
    cwd=root, env=env, capture_output=True, text=True, timeout=30)
assert failed.returncode != 0 and 'Failed to delete from server.ts' in failed.stderr
assert before == {str(p.relative_to(app)): p.read_bytes() for p in owned.rglob('*') if p.is_file()}
assert manifest_before == (app / '.forklaunch/manifest.toml').read_bytes()
server.write_text(original)
run('delete', 'router', 'profiles', '--continue', path=app / 'modules/typescript-api')
assert 'profilesRouter' not in server.read_text()
assert not (owned / 'api/routes/profiles.routes.ts').exists()
print('PASS async TypeScript router removal and preservation on unsupported server edits', flush=True)
for kind, name in [('service', 'typescript-api'), ('worker', 'typescript-jobs'), ('library', 'typescript-models')]:
    run('delete', kind, name, '--continue')
    assert not (app / 'modules' / name).exists()
    assert name not in {project['name'] for project in manifest()['projects']}
    assert manifest()['language'] == 'mixed'
    assert manifest()['applicationLanguage'] == 'mixed'
    for native in ['native-api', 'native-jobs', 'native-models']:
        assert (app / native_paths[native] / 'Cargo.toml').exists()
for kind, name in [('service', 'native-api'), ('worker', 'native-jobs'), ('library', 'native-models')]:
    run('delete', kind, name, '--continue')
    assert not (app / native_paths[name]).exists()
assert manifest()['runtime'] == 'node'
assert rust_image()['binaries'] == []
print('PASS actual mixed TypeScript/Rust service, worker, library and router lifecycle dispatch', flush=True)
print(f'Mixed lifecycle fixture evidence: {root}', flush=True)
if os.environ.get('FORKLAUNCH_MIXED_LIFECYCLE_RECEIPT'):
    Path(os.environ['FORKLAUNCH_MIXED_LIFECYCLE_RECEIPT']).write_text(json.dumps({
        'schemaVersion': 1, 'cliSha256': cli_hash, 'passed': True,
        'scope': 'Actual raw neutral init and generated mixed metadata; per-language sync, rename and delete dispatch; protected application sources; consumer-only native worker; shared image input isolation; TypeScript router unsupported-edit rejection with byte preservation',
        'compiled': False, 'scriptSha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    }, indent=2) + '\n')
shutil.rmtree(root)
account_path.unlink(missing_ok=True)

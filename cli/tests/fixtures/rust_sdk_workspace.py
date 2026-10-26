#!/usr/bin/env python3
"""Build real TypeScript-first mixed workspaces with PNPM and Bun after SDK generation."""
import json
import hashlib
import os
from pathlib import Path
import subprocess
import shutil
import shlex
import sys
import tempfile
import tomllib

repo = Path(__file__).resolve().parents[3]
binary = Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch')).resolve()
socket = os.environ.get('FORKLAUNCH_TEST_SOCKET_WORKSPACE') == '1'
socket_source = 'all' if os.environ.get('FORKLAUNCH_TEST_COMBINED_SDK') == '1' else 'socket-contract'
output = repo / 'cli/tests/output' / ('rust-socket-workspace' if socket else 'rust-sdk-workspace')
output.mkdir(parents=True, exist_ok=True)
root = Path(tempfile.mkdtemp(prefix='run.', dir=output)).resolve()
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'FORKLAUNCH_ACCOUNT': 'parity-offline-sdk-workspace',
       'CARGO_INCREMENTAL': '0', 'RUSTUP_TOOLCHAIN': 'stable'}
env.setdefault('CARGO_TARGET_DIR', str(repo / 'cli/target'))
if not env.get('FORKLAUNCH_WORKER_RUNTIME_LOCKED'):
    shutil.rmtree(root)
    sys.exit(subprocess.run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'),
        '-c', shlex.join([sys.executable, str(Path(__file__).resolve())])], env=env).returncode)
shutil.copy2(binary, root / 'forklaunch')
binary = root / 'forklaunch'
print('CLI SHA256:', hashlib.sha256(binary.read_bytes()).hexdigest(), flush=True)
results = []
for runtime in os.environ.get('FORKLAUNCH_TEST_WORKSPACE_RUNTIMES', 'node,bun').split(','):
    assert runtime in ['node', 'bun']
    app = root / runtime
    modules = 'modules' if runtime == 'node' else 'src'
    sdk_base = 'sdks' if modules == 'modules' else 'src/client-sdk'
    sdk_source = 'src' if modules == 'modules' else '.'
    def cli(*args):
        subprocess.run([str(binary), *args, '--path', str(app)], cwd=root, env=env, check=True, timeout=180)
    cli('init', 'application', runtime + '-workspace', '--modules-path', modules,
        '--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint',
        '--http-framework', 'express', '--runtime', runtime, '--test-framework', 'vitest',
        '-D', 'SDK workspace fixture', '-A', 'ForkLaunch', '-L', 'MIT')
    cli('init', 'service', 'legacy-api', '--language', 'typescript', '-d', 'postgresql', '-D', 'Legacy API')
    cli('init', 'service', 'native-api', '--language', 'rust', '-D', 'Native API')
    # Ordinary compilation supplies the current service-local HTTP contract and
    # both clients. The socket fixture below is authored compiler input; wire
    # behavior is covered by the separate WebSocket runtime suites.
    subprocess.run(['cargo', 'build', '-p', 'native-api'], cwd=app, env=env, check=True, timeout=1800)
    native_sdk = app / modules / 'native-api/sdk/typescript'
    assert (native_sdk / 'index.ts').read_text().strip() == "export type * from './sdk.js';"
    def sources():
        result = {}
        for p in (app / modules).rglob('*'):
            relative = p.relative_to(app / modules)
            if not p.is_file() or 'sdk' in relative.parts or relative.parts[0] == 'client-sdk':
                continue
            # SDK repair owns nested package membership, not domain sources.
            if relative.as_posix() in ['pnpm-workspace.yaml', 'package.json']:
                continue
            result[str(relative)] = p.read_bytes()
        return result
    before = sources()
    (app / 'package.json').write_text(json.dumps({'private': True, 'scripts': {'owned': 'echo application-owned'}}))
    (app / 'pnpm-workspace.yaml').write_text("packages: ['tools/*']\nminimumReleaseAge: 1440\n")
    if socket:
        artifact = app / modules / 'native-api/sdk/typescript/socket-contract.json'
        artifact.parent.mkdir(parents=True, exist_ok=True)
        artifact.write_bytes((repo / 'cli/tests/fixtures/rust_socket_contract.json').read_bytes())
        cli('sdk', 'generate', '--target', 'typescript', '--from', socket_source, '--service', 'native-api')
        cli('sdk', 'generate', '--target', 'typescript', '--from', socket_source, '--service', 'native-api', '--check')
        workspace = app / 'pnpm-workspace.yaml'
        workspace.write_text(''.join(line for line in workspace.read_text().splitlines(keepends=True) if '/typescript-websocket/' not in line))
        stale = workspace.read_bytes()
        package_before_check = (app / 'package.json').read_bytes()
        checked = subprocess.run([str(binary), 'sdk', 'generate', '--target', 'typescript', '--from', socket_source, '--service', 'native-api', '--check', '--path', str(app)], cwd=root, env=env, capture_output=True, text=True, timeout=180)
        assert checked.returncode != 0 and 'out of date' in checked.stderr + checked.stdout
        assert workspace.read_bytes() == stale and (app / 'package.json').read_bytes() == package_before_check
        cli('sdk', 'generate', '--target', 'typescript', '--from', socket_source, '--service', 'native-api')
    else:
        cli('sdk', 'generate', '--target', 'typescript', '--from', 'openapi', '--service', 'native-api')
    if socket and socket_source == 'all':
        assert (native_sdk / 'index.ts').is_file()
        assert (app / sdk_base / 'typescript-websocket/native-api' / sdk_source / 'index.ts').is_file()
    assert before == sources(), 'SDK workspace repair modified application sources'
    if runtime == 'bun':
        policy_path = app / 'bunfig.toml'
        policy = tomllib.loads(policy_path.read_text())
        original_policy = tomllib.loads((app / modules / 'bunfig.toml').read_text())
        assert policy['install']['minimumReleaseAge'] == 86400
        assert policy['install']['minimumReleaseAgeExcludes'] == original_policy['install']['minimumReleaseAgeExcludes']
        policy_path.write_text('# application policy\n' + policy_path.read_text().replace('minimumReleaseAge = 86400', 'minimumReleaseAge = 172800'))
        cli('sdk', 'generate', '--target', 'typescript', '--from', socket_source if socket else 'openapi', '--service', 'native-api')
        assert policy_path.read_text().startswith('# application policy\n')
        assert tomllib.loads(policy_path.read_text())['install']['minimumReleaseAge'] == 172800
    package = json.loads((app / 'package.json').read_text())
    assert package['scripts']['owned'] == 'echo application-owned'
    assert 'typescript' in package['devDependencies'] and 'tsx' in package['devDependencies']
    manager = 'bun' if runtime == 'bun' else 'pnpm'
    subprocess.run([manager, 'install', '--ignore-scripts'], cwd=app, env=env, check=True, timeout=300)
    if runtime == 'node':
        listed = json.loads(subprocess.check_output(['pnpm', 'list', '--recursive', '--depth', '-1', '--json'], cwd=app, env=env))
        names = {p.get('name') for p in listed}
        assert {f'@{runtime}-workspace/{name}' for name in ['core', 'monitoring', 'legacy-api', 'client-sdk']} <= names
        assert json.loads((native_sdk / 'package.json').read_text())['name'] in names
        if socket:
            assert 'native-api-websocket-sdk' in names
    else:
        expected = {f'{modules}/*', f'{modules}/*/sdk/typescript'}
        if socket:
            expected.add(sdk_base + '/typescript-websocket/*')
        assert expected <= set(package['workspaces'])
    # SDK checking must work on a clean install before any application build
    # has supplied declaration files for its shared runtime dependency.
    if not socket:
        subprocess.run([manager, 'run', 'sdk:typecheck'], cwd=app, env=env, check=True, timeout=1800)
    build = subprocess.run([manager, 'run', 'build'], cwd=app, env=env, capture_output=True, text=True, timeout=1800)
    print(build.stdout, flush=True)
    print(build.stderr, flush=True)
    build.check_returncode()
    if runtime == 'bun':
        for name in ['core', 'monitoring', 'legacy-api', 'client-sdk']:
            assert (build.stdout + build.stderr).count(f'/{modules}/{name}: bun run build') == 1, f'{name} must build exactly once'
    for name in ['core', 'monitoring', 'legacy-api', 'client-sdk']:
        assert any((app / modules / name).rglob('*.tsbuildinfo')), f'{name} was omitted from root build'
    subprocess.run(['node', str(repo / 'cli/tests/fixtures/rust_typescript_service_startup.mjs'), str(app), modules], cwd=app, env=env, check=True, timeout=120)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=app, env=env))
    subprocess.run(['node', str(repo / 'cli/tests/fixtures/rust_mcp_service_startup.mjs'), str(Path(metadata['target_directory']) / 'debug/native-api')], cwd=app, env=env, check=True, timeout=120)
    print(f'PASS {runtime} root workspace installation/build includes TypeScript modules and SDK; both languages answer HTTP/MCP', flush=True)
    results.append({'runtime': runtime, 'modulesPath': modules, 'source': socket_source if socket else 'openapi', 'passed': True})
print(f'SDK workspace fixture evidence: {root}', flush=True)
if os.environ.get('FORKLAUNCH_SDK_WORKSPACE_RECEIPT'):
    Path(os.environ['FORKLAUNCH_SDK_WORKSPACE_RECEIPT']).write_text(json.dumps({
        'schemaVersion': 1, 'cliSha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'cases': results,
        'checks': ['neutral raw initialization', 'compile-generated service-local SDKs', 'application source preservation',
                   'workspace metadata repair and check purity', 'Bun installation policy preservation',
                   'actual package installation and root build', 'Bun modules build exactly once', 'both languages HTTP/MCP'],
        'socketScope': 'Authored compiler fixture; wire transport belongs to separate runtime suites' if socket else None,
    }, indent=2) + '\n')

# Retain generated source and evidence without accumulating per-run dependencies.
# Opt in to keeping successful-run caches when debugging the generated workspace.
if os.environ.get('FORKLAUNCH_KEEP_TEST_CACHES') != '1':
    shutil.rmtree(root)
    print('Removed completed fixture sources and dependency caches; validation receipt retained', flush=True)

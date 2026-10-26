#!/usr/bin/env python3
"""App-wide native/TypeScript discovery and shared SDK export through the CLI."""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import sys
import subprocess
import tempfile
import tomllib

repo = Path(__file__).resolve().parents[3]
binary = Path(os.environ.get('FORKLAUNCH_CLI', repo / 'cli/target/debug/forklaunch')).resolve()
output = repo / 'cli/tests/output/rust-mixed-sync'
output.mkdir(parents=True, exist_ok=True)
root = Path(tempfile.mkdtemp(prefix='run.', dir=output)).resolve()
app = root / 'app'
env = {**os.environ, 'FORKLAUNCH_SKIP_FORMAT': '1', 'CARGO_INCREMENTAL': '0',
       'RUSTUP_TOOLCHAIN': 'stable'}
env.setdefault('CARGO_TARGET_DIR', str(repo / 'cli/target'))
if not env.get('FORKLAUNCH_WORKER_RUNTIME_LOCKED'):
    shutil.rmtree(root)
    sys.exit(subprocess.run([sys.executable, str(repo / 'cli/tests/fixtures/worker_runtime_lock.py'),
        '-c', shlex.join([sys.executable, str(Path(__file__).resolve())])], env=env).returncode)


print(f'CLI SHA256: {hashlib.file_digest(binary.open("rb"), "sha256").hexdigest()}', flush=True)

def run(*args):
    subprocess.run([str(binary), *args, '--path', str(app)], cwd=root, env=env, check=True, timeout=1800)


try:
    run('init', 'application', 'mixed-sync', '--modules-path', 'modules',
        '--database', 'postgresql', '--validator', 'zod', '--formatter', 'prettier', '--linter', 'oxlint',
        '--http-framework', 'express', '--runtime', 'node', '--test-framework', 'vitest',
        '-D', 'Mixed sync', '-A', 'ForkLaunch', '-L', 'MIT')
    run('init', 'service', 'legacy-api', '--language', 'typescript', '-d', 'postgresql', '-D', 'Legacy API')
    run('init', 'library', 'discovered-models', '--language', 'typescript', '-D', 'Discovered models')
    run('init', 'service', 'native-api', '--language', 'rust', '-D', 'Native API')
    run('init', 'library', 'native-models', '--language', 'rust', '-D', 'Native models')
    manifest_path = app / '.forklaunch/manifest.toml'
    # Both language exporters must refresh the registered custom OpenAPI paths.
    manifest_text = re.sub(r'^openapiPath = .*\n', '', manifest_path.read_text(), flags=re.M)
    blocks = manifest_text.split('[[projects]]')
    for name in ['legacy-api', 'native-api']:
        for index, block in enumerate(blocks):
            if f'\nname = "{name}"\n' not in block:
                continue
            if '[projects.metadata]\n' not in block:
                boundary = re.search(r'^\[', block, flags=re.M)
                offset = boundary.start() if boundary else len(block)
                block = block[:offset] + '[projects.metadata]\n\n' + block[offset:]
            blocks[index] = block.replace('[projects.metadata]\n',
                f'[projects.metadata]\nopenapiPath = "schemas/{name}.json"\n', 1)
            break
        else:
            raise AssertionError(f'Missing {name}')
    manifest_text = '[[projects]]'.join(blocks)
    manifest_path.write_text(manifest_text)
    # Remove only this library's registration while leaving the actual TS scaffold.
    text = manifest_path.read_text()
    starts = [match.start() for match in re.finditer(r'^\[\[projects\]\]$', text, flags=re.M)]
    for start, end in zip(starts, starts[1:] + [len(text)]):
        block = text[start:end]
        if '\nname = "discovered-models"\n' in block:
            other_table = re.search(r'^\[(?!\[?projects[.\]])', block, flags=re.M)
            stop = start + other_table.start() if other_table else end
            text = text[:start] + text[stop:]
            break
    else:
        raise AssertionError('Missing library registration')
    assert 'discovered-models' not in {p['name'] for p in tomllib.loads(text)['projects']}
    # Keep consumer aliases/features while repairing a relocated local dependency.
    consumer_path = app / 'modules/native-api/Cargo.toml'
    consumer_text = consumer_path.read_text()
    # init library writes the consumer dependency inline
    # (`native-models = { path = "..." }`); older CLIs wrote a table.
    if '[dependencies.native-models]' in consumer_text:
        consumer_text = consumer_text.replace('[dependencies.native-models]', '[dependencies.models]\npackage = \"native-models\"\nfeatures = [\"custom-models\"]')
    else:
        inline = re.search(r'^native-models = \{ path = ("[^"]+") \}$', consumer_text, flags=re.M)
        assert inline, consumer_text
        consumer_text = consumer_text.replace(inline.group(0), f'models = {{ package = "native-models", path = {inline.group(1)}, features = ["custom-models"] }}')
    consumer_path.write_text(consumer_text)
    library_path = app / 'modules/core/native-models/Cargo.toml'
    library_path.write_text(library_path.read_text() + '\n[features]\ncustom-models = []\n')
    # A registered native library can live outside the usual modules/core tree.
    # Preserve its location and custom source while repairing the missing Cargo member.
    (app / 'shared').mkdir()
    old_library = app / 'modules/core/native-models'
    new_library = app / 'shared/native-models'
    manifest_source = (old_library / 'Cargo.toml').read_text()
    for section in ['dependencies', 'dev-dependencies', 'build-dependencies']:
        for dependency in tomllib.loads(manifest_source).get(section, {}).values():
            if isinstance(dependency, dict) and 'path' in dependency:
                old_path = dependency['path']
                new_path = os.path.relpath((old_library / old_path).resolve(), new_library)
                manifest_source = manifest_source.replace(json.dumps(old_path), json.dumps(new_path))
    (old_library / 'Cargo.toml').write_text(manifest_source)
    old_library.rename(new_library)
    text = text.replace('modules/core/native-models', 'shared/native-models')
    manifest_path.write_text(text)
    cargo = app / 'Cargo.toml'
    cargo.write_text(re.sub(r',\s*"modules/core/native-models"', '', cargo.read_text()))
    source = app / 'shared/native-models/lib.rs'
    source.write_text(source.read_text() + '\npub const USER_OWNED: bool = true;\n')
    original = source.read_bytes()
    # Use a real tsx export process with an owned export hook; no framework/DB startup
    # is required just to prove language dispatch and artifact refresh.
    tools = root / 'tools'
    tools.mkdir()
    (tools / 'package.json').write_text('{"private":true}')
    subprocess.run(['npm', 'install', '--ignore-scripts', '--no-audit', '--no-fund', 'tsx@4.23.15'],
        cwd=tools, env=env, check=True, timeout=180)
    service = app / 'modules/legacy-api'
    (service / 'node_modules').symlink_to(tools / 'node_modules', target_is_directory=True)
    (service / 'server.ts').write_text('''import {mkdirSync,writeFileSync} from 'node:fs';
    if(process.env.FORKLAUNCH_MODE!=='openapi') throw new Error('Only export is expected');
    const schema={type:'object',properties:{origin:{type:'string'}},required:['origin']};
    writeFileSync(process.env.FORKLAUNCH_OPENAPI_OUTPUT!,JSON.stringify({openapi:'3.1.0',info:{title:'Legacy API',version:'1'},
     paths:{'/legacy':{get:{operationId:'getLegacy',responses:{'200':{description:'OK',content:{'application/json':{schema}}}}}}}}));
    mkdirSync('sdk',{recursive:true});
    writeFileSync('sdk/contract.json',JSON.stringify({formatVersion:'forklaunch.contract.v1',sdkCompiler:'forklaunch.sdk.http.v1',generator:'framework',
      service:{name:'legacy-api',version:'1'},routes:[{name:'Legacy API Get',operationName:'getLegacy',method:'GET',
      path:'/legacy',openapiPath:'/legacy',access:'public',request:{params:[],query:[],headers:[]},
      responses:[{statusCode:200,headers:[],body:{kind:'json',contentType:'application/json',schema:{name:'LegacyResponse',jsonSchema:schema}}}]}]}));
    ''')
    run('sync', 'all', '--confirm')
    manifest = tomllib.loads(manifest_path.read_text())
    projects = {project['name']: project for project in manifest['projects']}
    assert {'legacy-api', 'native-api', 'native-models', 'discovered-models'} <= projects.keys()
    assert projects['native-models']['metadata']['cratePath'] == 'shared/native-models'
    assert source.read_bytes() == original
    dependency = tomllib.loads(consumer_path.read_text())['dependencies']['models']
    assert dependency['package'] == 'native-models' and dependency['features'] == ['custom-models']
    assert (consumer_path.parent / dependency['path']).resolve() == new_library.resolve()
    assert 'shared/native-models' in tomllib.loads(cargo.read_text())['workspace']['members']
    assert (app / 'modules/native-api/Cargo.toml').is_file()
    assert not (app / 'modules/native-models').exists()
    assert (app / 'modules/native-api/sdk/typescript/contract.json').is_file()
    # Sync refreshes each service's registered document; no shared export folder.
    assert not (app / '.forklaunch/openapi').exists()
    for name in ['legacy-api', 'native-api']:
        assert json.loads((app / f'schemas/{name}.json').read_text())['paths']
    sdk = (app / 'modules/client-sdk/clientSdk.ts').read_text()
    assert (app / 'modules/native-api/sdk/typescript/sdk.ts').is_file()
    assert (app / 'modules/native-api/sdk/rust/Cargo.toml').is_file()
    assert (app / 'modules/legacy-api/sdk/rust/Cargo.toml').is_file()
    assert any('get_legacy' in path.read_text() for path in (app / 'modules/legacy-api/sdk/rust').rglob('*.rs'))
    assert 'native' in sdk.lower() and 'legacy' in sdk.lower(), sdk
    run('sdk', 'generate', '--from', 'contract', '--target', 'all', '--check')
    run('sdk', 'generate', '--from', 'openapi', '--target', 'all')
    run('sdk', 'generate', '--from', 'openapi', '--target', 'all', '--check')
    # Explicit output retains the requested layout and still refreshes registered artifacts.
    for name in ['legacy-api', 'native-api']:
        (app / f'schemas/{name}.json').write_text('stale')
    run('openapi', 'export', '--output', 'release-openapi')
    for name in ['legacy-api', 'native-api']:
        assert (app / f'schemas/{name}.json').read_bytes() == (app / f'release-openapi/{name}/openapi.json').read_bytes()
    run('sdk', 'generate', '--from', 'contract', '--target', 'all')
    subprocess.run(['cargo', 'check', '--workspace', '--all-targets'], cwd=app, env=env, check=True, timeout=1800)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=app, env=env))
    subprocess.run(['node', str(repo / 'cli/tests/fixtures/rust_mcp_service_startup.mjs'),
        str(Path(metadata['target_directory']) / 'debug/native-api')], cwd=app, env=env, check=True, timeout=120)
    unknown = app / 'modules/unknown-project'
    unknown.mkdir()
    failed = subprocess.run([str(binary), 'sync', 'all', '--confirm', '--path', str(app)],
        cwd=root, env=env, capture_output=True, text=True, timeout=180)
    assert failed.returncode == 0 and "Skipping 'unknown-project'" in failed.stderr + failed.stdout
    assert 'panicked' not in failed.stderr
    unknown.rmdir()
    # Removal on one side must not classify the other language as orphaned.
    shutil.rmtree(app / 'shared/native-models')
    run('sync', 'all', '--confirm')
    projects = {project['name']: project for project in tomllib.loads(manifest_path.read_text())['projects']}
    assert 'native-models' not in projects
    assert {'legacy-api', 'native-api', 'discovered-models'} <= projects.keys()
    assert 'native-models' not in (app / 'modules/native-api/Cargo.toml').read_text()
    run('sdk', 'generate', '--from', 'contract', '--target', 'all', '--check')
    print('PASS mixed app-wide sync: TypeScript discovery, custom native paths, independent orphan cleanup and both SDK targets', flush=True)
    # Selecting Rust output must preserve the existing TypeScript declarations.
    typescript = {p: p.read_bytes() for p in (app / 'modules/native-api/sdk/typescript').rglob('*') if p.is_file()}
    run('sdk', 'generate', '--from', 'contract', '--target', 'rust')
    assert all(p.read_bytes() == content for p, content in typescript.items())
    run('sdk', 'generate', '--from', 'contract', '--target', 'rust', '--check')
    print('PASS explicit Rust SDK generation preserves live TypeScript exports', flush=True)
finally:
    if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):
        print(f'Mixed sync fixture evidence: {root}', flush=True)
    else:
        shutil.rmtree(root)

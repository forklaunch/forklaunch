#!/usr/bin/env python3
"""Actual mixed application tooling changes keep native projects and build hooks intact."""
import hashlib,json,os,pathlib,shlex,shutil,subprocess,sys,tempfile,tomllib
repo=pathlib.Path(__file__).resolve().parents[3]
cli=pathlib.Path(os.environ.get('FORKLAUNCH_CLI',repo/'cli/target/debug/forklaunch'))
root=pathlib.Path(tempfile.mkdtemp(prefix='forklaunch-mixed-tooling-'));app=root/'app'
env={**os.environ,'FORKLAUNCH_SKIP_FORMAT':'1','CARGO_INCREMENTAL':'0','CARGO_TARGET_DIR':str(repo/'cli/target')}
def run(args,cwd=root):
 r=subprocess.run(args,cwd=cwd,env=env,capture_output=True,text=True);assert r.returncode==0,r.stdout[-5000:]+r.stderr[-7000:];return r.stdout
def command(*args):return run([str(cli),*args,'--path',str(app)])
try:
 command('init','application','mixed-tooling','--language','typescript','--modules-path','modules','--database','postgresql','--validator','zod','--formatter','prettier','--linter', 'oxlint','--http-framework','express','--runtime','node','--test-framework','jest','-D','Tooling','-A','ForkLaunch','-L','MIT')
 command('init','service','api','--language','typescript','-d','postgresql','-D','API')
 command('init','library','models','--language','rust','-D','Models')
 manifest=tomllib.loads((app/'.forklaunch/manifest.toml').read_text());native=next(p for p in manifest['projects'] if p['name']=='models');directory=app/native['metadata']['cratePath']
 files={p:p.read_bytes() for p in directory.rglob('*') if p.is_file()};cargo=(app/'Cargo.toml').read_bytes()
 hook=json.loads((app/'modules/api/package.json').read_text())['scripts']['build']
 command('change','application','--formatter','biome','--linter','oxlint','--test-framework','vitest','--confirm')
 changed=tomllib.loads((app/'.forklaunch/manifest.toml').read_text())
 assert changed['language']=='mixed' and changed['formatter']=='biome' and changed['linter']=='oxlint' and changed['test_framework']=='vitest'
 assert next(p for p in changed['projects'] if p['name']=='models')==native
 assert all(p.read_bytes()==value for p,value in files.items()) and (app/'Cargo.toml').read_bytes()==cargo
 package=json.loads((app/'modules/api/package.json').read_text());assert package['scripts']['build']==hook
 assert 'vitest' in package['scripts']['test']
 # Hook customizations must fail before registrations, dependencies or manifest mutate.
 config=app/'modules/api/package.json';original=config.read_bytes();scripts=json.loads(original);scripts['scripts']['sdk:build']+=' --customized';config.write_text(json.dumps(scripts))
 before={str(p.relative_to(app)):hashlib.sha256(p.read_bytes()).hexdigest() for p in app.rglob('*') if p.is_file()}
 rejected=subprocess.run([str(cli),'change','application','--validator','typebox','--confirm','--path',str(app)],env=env,capture_output=True,text=True)
 assert rejected.returncode!=0 and 'customized' in (rejected.stdout+rejected.stderr).lower()
 assert before=={str(p.relative_to(app)):hashlib.sha256(p.read_bytes()).hexdigest() for p in app.rglob('*') if p.is_file()}
 config.write_bytes(original)
 if os.environ.get('FORKLAUNCH_ADAPTER_MIGRATION')=='hyper':
  before={str(p.relative_to(app)):hashlib.sha256(p.read_bytes()).hexdigest() for p in app.rglob('*') if p.is_file()}
  rejected=subprocess.run([str(cli),'change','application','--http-framework','hyper-express','--confirm','--path',str(app)],env=env,capture_output=True,text=True)
  assert rejected.returncode!=0 and 'pnpm policy' in rejected.stdout+rejected.stderr
  assert before=={str(p.relative_to(app)):hashlib.sha256(p.read_bytes()).hexdigest() for p in app.rglob('*') if p.is_file()}
  # Explicit supported-toolchain choice; no dependency admission setting changes.
  path=app/'modules/package.json';package=json.loads(path.read_text());package['packageManager']='pnpm@10.33.2';path.write_text(json.dumps(package,indent=2)+'\n')
  command('change','application','--validator','typebox','--http-framework','hyper-express','--confirm')
  assert tomllib.loads((app/'.forklaunch/manifest.toml').read_text())['http_framework']=='hyper-express'
 else:
  command('change','application','--validator','typebox','--confirm')
 # The validator is an application setting; the service carries it as a dependency.
 assert tomllib.loads((app/'.forklaunch/manifest.toml').read_text())['validator']=='typebox'
 assert '@sinclair/typebox' in json.loads(config.read_text())['dependencies']
 assert all(p.read_bytes()==value for p,value in files.items())
 run(['pnpm','install','--ignore-scripts'],app/'modules')
 sample=app/'modules/api/tooling.test.ts';sample.write_text("import { expect, test } from 'vitest'; test('new runner',()=>expect(2+2).toBe(4));\n")
 run(['pnpm','test','--run','tooling.test.ts'],app/'modules/api');sample.unlink()
 run([sys.executable,str(repo/'cli/tests/fixtures/worker_runtime_lock.py'),'-c',shlex.join(['pnpm','build'])],app/'modules')
 run([sys.executable,str(repo/'cli/tests/fixtures/worker_runtime_lock.py'),'-c',shlex.join(['cargo','+stable','test','--manifest-path',str(app/'Cargo.toml'),'-p','models'])],repo)
 print('PASS mixed formatter/linter/test conversion preserves native sources/metadata and SDK hooks; validator hook migration/customization preservation, selected TS tests, dual-SDK build and Rust consumer pass',flush=True)
finally:
 if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):print('Fixture retained:',root,flush=True)
 else:shutil.rmtree(root)

#!/usr/bin/env python3
"""Owned Node/Bun runtime migration preserves native projects and dual-SDK hooks."""
import hashlib,json,os,pathlib,shlex,shutil,subprocess,sys,tempfile,tomllib
repo=pathlib.Path(__file__).resolve().parents[3]
cli=pathlib.Path(os.environ.get('FORKLAUNCH_CLI',repo/'cli/target/debug/forklaunch'))
root=pathlib.Path(tempfile.mkdtemp(prefix='forklaunch-runtime-migration-'));app=root/'app'
env={**os.environ,'FORKLAUNCH_SKIP_FORMAT':'1','CARGO_INCREMENTAL':'0','CARGO_TARGET_DIR':str(repo/'cli/target')}
def run(args,cwd=root,check=True):
 r=subprocess.run(args,cwd=cwd,env=env,capture_output=True,text=True)
 if check:assert r.returncode==0,r.stdout[-5000:]+r.stderr[-9000:]
 return r
def command(*args,check=True):return run([str(cli),*args,'--path',str(app)],check=check)
def tree():return {str(p.relative_to(app)):hashlib.sha256(p.read_bytes()).hexdigest() for p in app.rglob('*') if p.is_file()}
try:
 command('init','application','runtime-migration','--language','typescript','--modules-path','modules','--database','postgresql','--validator','zod','--formatter','prettier','--linter', 'oxlint','--http-framework','express','--runtime','node','--test-framework','vitest','-D','Runtime migration','-A','ForkLaunch','-L','MIT')
 command('init','service','api','--language','typescript','-d','postgresql','-D','API')
 command('init','library','models','--language','rust','-D','Native')
 metadata=tomllib.loads((app/'.forklaunch/manifest.toml').read_text());native=app/next(p['metadata']['cratePath'] for p in metadata['projects'] if p['name']=='models');saved={p:p.read_bytes() for p in native.rglob('*') if p.is_file()}
 compose=app/'docker-compose.yaml';original=compose.read_bytes();compose.write_text(original.decode().replace('/production/server.mjs','/production/custom.mjs'))
 before=tree();rejected=command('change','application','--runtime','bun','--confirm',check=False)
 assert rejected.returncode!=0 and 'Customized production process' in rejected.stdout+rejected.stderr
 assert tree()==before
 compose.write_bytes(original)
 package_path=app/'modules/package.json';package_original=package_path.read_bytes();package=json.loads(package_original)
 package['scripts']['clean:purge']='node -e "require(\'node:fs\').writeFileSync(\'purged\', \'unexpected\')"'
 package_path.write_text(json.dumps(package,indent=2)+'\n')
 before=tree();command('change','application','--runtime','bun','-N','renamed','--confirm','--dryrun');assert tree()==before
 package_path.write_bytes(package_original)
 for runtime,pm in [('bun','bun'),('node','pnpm')]:
  command('change','application','--runtime',runtime,'--confirm')
  print('PASS generated runtime migration:',runtime,flush=True)
  manifest=tomllib.loads((app/'.forklaunch/manifest.toml').read_text());assert manifest['runtime']==runtime and manifest['language']=='mixed' and manifest['test_framework']=='vitest'
  assert all(p.read_bytes()==content for p,content in saved.items())
  api=next(p for p in manifest['projects'] if p['name']=='api');assert api['metadata']['contractBuild'] is True
  package=json.loads((app/'modules/api/package.json').read_text());assert package['scripts']['build']=='tsc -b'
  docker=(app/'Dockerfile.typescript').read_text();assert ('FROM oven/bun:' in docker)==(runtime=='bun')
  assert package['scripts']['start:production:server']==runtime+' --enable-source-maps production/server.mjs'
  assert 'build:production' not in json.loads((app/'modules/package.json').read_text())['scripts']
  assert (f'RUN {runtime} /forklaunch/production-build.mjs /forklaunch/production.json') in docker
  assert package['scripts']['test'].startswith('bun run --bun' if runtime=='bun' else 'pnpm')
  assert package['scripts']['retention:enforce']==('bun run scripts/enforce-retention.ts' if runtime=='bun' else 'pnpm tsx scripts/enforce-retention.ts')
  run([pm,'install','--ignore-scripts'],app/'modules')
  run([sys.executable,str(repo/'cli/tests/fixtures/worker_runtime_lock.py'),'-c',shlex.join([pm,'run','build'])],app/'modules')
  # Services build with plain tsc; the CLI compiles contract SDKs separately
  # (the service's `sdk:build` script runs this same command).
  command('sdk','build','--service','api')
  assert (app/api['metadata']['contractArtifactPath']).is_file()
  print('PASS normal dual-SDK build:',runtime,flush=True)
  sample=app/'modules/api/runtime.test.ts';sample.write_text("import {expect,test} from 'vitest';test('selected runner',()=>expect(4).toBe(4));\n")
  run([pm,'run','test','--run','runtime.test.ts'],app/'modules/api');sample.unlink()
 print('PASS Node/Bun roundtrip preserves native files and custom process; dryrun writes nothing; selected runner and normal dual-SDK builds pass',flush=True)
finally:
 if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):print('Fixture retained:',root,flush=True)
 else:shutil.rmtree(root)

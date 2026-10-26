#!/usr/bin/env python3
"""Conversion rejects customized lifecycle source before any rename/write."""
import hashlib,json,os,pathlib,shlex,shutil,subprocess,sys,tempfile,tomllib
repo=pathlib.Path(__file__).resolve().parents[3];cli=pathlib.Path(os.environ.get('FORKLAUNCH_CLI',repo/'cli/target/debug/forklaunch'))
root=pathlib.Path(tempfile.mkdtemp(prefix='forklaunch-conversion-proof-'));app=root/'app'
env={**os.environ,'FORKLAUNCH_SKIP_FORMAT':'1','CARGO_INCREMENTAL':'0','CARGO_TARGET_DIR':str(repo/'cli/target')}
def run(args,cwd=root,check=True):
 r=subprocess.run(args,cwd=cwd,env=env,capture_output=True,text=True)
 if check: assert r.returncode==0,r.stdout[-6000:]+r.stderr[-9000:]
 return r
def command(*args,path=app,check=True):return run([str(cli),*args,'--path',str(path)],check=check)
def tree():return {str(p.relative_to(app)):hashlib.sha256(p.read_bytes()).hexdigest() for p in app.rglob('*') if p.is_file()}
try:
 command('init','application','conversion-proof','--language','rust','-D','Conversions','-A','ForkLaunch','-L','MIT')
 command('init','worker','jobs','--language','rust','-t','database','-d','postgresql','-D','Jobs')
 command('init','service','api','--language','rust','-D','API')
 run([sys.executable,str(repo/'cli/tests/fixtures/worker_runtime_lock.py'),'-c',shlex.join(['cargo','+stable','check','--manifest-path',str(app/'Cargo.toml'),'-p','api','-p','jobs'])],repo)
 # Since e3842817d each project owns its SDK under sdk/; builds emit their HTTP contracts there, which
 # renames require to be current before they reach the lifecycle-customization checks below.
 assert (app/'src/api/sdk/typescript/contract.json').is_file()
  # Kind/backend changes replace only generated files that differ between the
 # two shapes and refuse when one of those is customized (ef39d81dd); pick a
 # file each change rewrites.
 for kind,name,extra,file in [('worker','jobs',['-t','redis'],'domain/services/producer.rs'),('service','api',['--to','worker','-t','database','-d','postgresql'],'lib.rs')]:
  directory=app/'src'/name;p=directory/file;original=p.read_bytes();p.write_bytes(original+b'\n// application-owned lifecycle customization\n')
  for dry in [[],['--dryrun']]:
   before=tree();result=command('change',kind,*extra,'-N','replacement','--confirm',*dry,path=directory,check=False)
   assert result.returncode != 0 and 'would overwrite customized' in result.stderr+result.stdout,result.stdout+result.stderr
   after=tree();assert after==before,('Rejected conversion changed application files',dry,sorted(k for k in set(before)|set(after) if before.get(k)!=after.get(k)))
  p.write_bytes(original)
  cargo=directory/'Cargo.toml';original_cargo=cargo.read_bytes();cargo.write_bytes(original_cargo+b'\n[features]\napplication_feature = []\n')
  before=tree();result=command('change',kind,*extra,'--confirm',path=directory,check=False)
  assert result.returncode != 0 and 'would overwrite customized' in result.stderr+result.stdout and 'Cargo.toml' in result.stderr+result.stdout,result.stdout+result.stderr
  assert tree()==before,'Rejected manifest conversion changed application files'
  cargo.write_bytes(original_cargo)
  command('change',kind,*extra,'--confirm',path=directory)
 assert not (app/'src/api/domain/Cargo.toml').exists()
 assert 'WORKER_DATABASE_URL' not in (app/'docker-compose.yaml').read_text().split('jobs:',1)[1].split('api:',1)[0]
 run([sys.executable,str(repo/'cli/tests/fixtures/worker_runtime_lock.py'),'-c',shlex.join(['cargo','+stable','test','--manifest-path',str(app/'Cargo.toml'),'-p','jobs','-p','api','--lib'])],repo)
 # No service-era SDK output survives conversion: once built, the converted
 # project's local SDK is exactly a worker's (same files, worker surface).
 sdk=lambda name:app/'src'/name/'sdk/typescript'
 assert sorted(p.name for p in sdk('api').iterdir())==sorted(p.name for p in sdk('jobs').iterdir())
 routes=lambda name:sorted((r['method'],r['path']) for r in json.loads((sdk(name)/'contract.json').read_text())['routes'])
 assert routes('api')==routes('jobs'),routes('api')
 print('PASS custom worker backend/service kind conversions preserve entire tree including rename/dryrun; unchanged defaults convert and compile',flush=True)
finally:
 if os.environ.get('FORKLAUNCH_KEEP_FIXTURE'):print('Fixture retained:',root,flush=True)
 else:shutil.rmtree(root)

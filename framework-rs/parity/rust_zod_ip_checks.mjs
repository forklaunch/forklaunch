import {actualValidator} from './rust_actual_validator_fixture.mjs';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {z}=require('zod/v3'),Ajv=require('ajv/dist/2020').default,addFormats=require('ajv-formats');
const {validator,load}=actualValidator(root,undefined,{sameRealm:true});
const {registerZodChecks}=load(path.join(root,'framework/universal-sdk/src/core/zodChecks.ts'));
const native=process.env.FORKLAUNCH_ZOD_CHECKS_NATIVE==='1';
const target=path.resolve(process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target'));
function run(command,args,options={}){
 const out=spawnSync(command,args,{cwd:root,env:{...process.env,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:'1'},encoding:'utf8',maxBuffer:64*1024*1024,...options});
 assert.equal(out.status,0,out.stdout+'\n'+out.stderr);return out.stdout;
}
if(native)run('cargo',['build','--locked','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--features','zod-checks','--example','uri_schema_parity']);
let total=0,nativeCount=0;
function check(schema,inputs){
 for(const pureInput of [false,true]){
  const baseline=validator.openapi2020(schema,{pureInput});
  assert.deepEqual(validator.openapi2020(schema,{pureInput,zodChecks:false}),baseline);
  const emitted=validator.openapi2020(schema,{pureInput,zodChecks:true});
  const engine=registerZodChecks(new Ajv({strict:false,ownProperties:true}));addFormats(engine);
  const validate=engine.compile(emitted),expected=inputs.map(v=>validator.parse(schema,v).ok);
  inputs.forEach((v,i)=>{assert.equal(!!validate(v),expected[i],JSON.stringify({emitted,v}));total++;});
  if(native){const actual=run(path.join(target,'debug/examples/uri_schema_parity'),[],{input:[emitted,...inputs].map(v=>JSON.stringify(v)).join('\n')+'\n'}).trim().split('\n').map(JSON.parse);assert.deepEqual(actual,expected);nativeCount+=actual.length;}
 }
}

const addresses=new Set(['','0.0.0.0','255.255.255.255','127.1','0x7f.0.0.1','1.2.3.4.5','::','::1','::ffff:192.168.00.01','::FFFF:192.168.1.1','::ffff:0000:192.168.1.1','1::192.168.1.1','1:2:3:4:5:6:192.168.1.1','fe80:%a','fe80::%a','fe80:::::%a','FE80::%a','fe80::1%eth0','fe80::1%eth-0','::%lo','１.２.３.４']);
for(let n=0;n<300;n++)for(const octet of [String(n),String(n).padStart(2,'0'),String(n).padStart(3,'0'),'-'+n,'+'+n])for(let position=0;position<4;position++){
 const parts=['1','2','3','4'];parts[position]=octet;const ip=parts.join('.');addresses.add(ip);
 for(const prefix of ['::','::ffff:','::FFFF:','::ffff:0000:','1::','1:2:3:4::','1:2:3:4:5::'])addresses.add(prefix+ip);
}
for(let count=0;count<=9;count++)for(const group of ['0','0000','1','aBcD','ffff','FFFF','12345','g','']){
 addresses.add(Array(count).fill(group).join(':'));
 for(let start=0;start<=count;start++)for(let end=start;end<=count;end++)addresses.add(Array(start).fill(group).join(':')+'::'+Array(count-end).fill(group).join(':'));
}
for(let count=0;count<=6;count++)for(const group of ['','1','0000','abcd','12345'])for(const zone of ['a','eth0','0','A1','a-b','','é'])addresses.add('fe80:'+Array(count).fill(':'+group).join('')+'%'+zone);
for(const value of ['1.2.3.4','::1','fe80::%a'])for(const suffix of ['\n','\r','\r\n','\u2028','\u2029','\0',' '])addresses.add(value+suffix);
const short=[...addresses];
const inputs=[...short,'fe80:%'+'a'.repeat(1000000),':'.repeat(1000000),null,1];
for(const version of [undefined,'v4','v6'])check(z.string().ip({version}),inputs);
const cidrs=[];
for(const address of [...short.slice(0,100),...short.filter(s=>s.includes('%')), '1.2.3.4','::','::ffff:01.02.03.04'])for(const width of ['','0','00','01','1','9','10','31','32','33','99','100','127','128','129','255','-1','+1','１','1.0','1/2'])cidrs.push(address+'/'+width);
for(let width=0;width<=140;width++)for(const address of ['1.2.3.4','::','fe80::%a'])cidrs.push(address+'/'+width);
for(const version of [undefined,'v4','v6'])check(z.string().cidr({version}),cidrs);
const edges=[...short.slice(0,60),...cidrs.slice(0,60)];
for(const schema of [z.string().ip().ip({version:'v4'}),z.string().ip({version:'v4'}).ip({version:'v6'}),z.string().cidr().cidr({version:'v6'}),z.string().ip().min(8),z.string().ip().startsWith('::')])check(schema,edges);
check(z.object({value:z.string().ip().optional()}),[{},...edges.map(value=>({value}))]);
check(z.array(z.string().cidr()),edges.map(value=>[value]));
check(z.union([z.string().ip(),z.literal('alternate')]),[...edges,'alternate']);
for(const method of ['ip','cidr']){
 for(const version of ['v5',1,null])assert.throws(()=>validator.openapi2020(z.string()[method]({version}),{zodChecks:true}),/version/);
 for(const format of ['uuid',undefined,null]){const out=validator.openapi2020(z.string()[method]().openapi({format}),{zodChecks:true});assert.equal(out.format,format);assert.equal(out['x-forklaunch-zod-checks'],undefined);}
 assert.throws(()=>validator.openapi2020(z.string().trim()[method](),{zodChecks:true}),/transformations/);
}
console.log(`PASS ${total} actual Zod/AJV IP/CIDR comparisons and ${nativeCount} native observations; versions, mixed legacy syntax, zones, prefix bounds, nesting and overrides`);

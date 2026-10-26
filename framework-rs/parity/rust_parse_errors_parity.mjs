import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const {load} = actualValidator(root);
const {parse: parseResponse} = load(path.join(root, 'framework/core/src/http/middleware/response/parse.middleware.ts'));
const {parse: parseRequest} = load(path.join(root, 'framework/core/src/http/middleware/request/parse.middleware.ts'));
const {prettyPrintParseErrors} = load(path.join(root, 'framework/validator/src/shared/utils/prettyPrintParseErrors.ts'));
const cases = [];
const prefixes = [undefined, null, '', 'Request', 'Session', 'Header', 'Response', ' 用户\n', '😀'];
const messages = ['', 'Required', 'Invalid input: expected string, received undefined', 'a\nb\r\nc', 'quotes: " \\ < >', 'é/用户/😀', '\0'];
const paths = [[], [''], ['root'], ['user', 'email'], ['items', '0', 'name'], [' > ', '\n'], ['é', '用户', '😀']];
for (const prefix of prefixes) {
  cases.push({prefix}, {prefix, errors: null}, {prefix, errors: []});
  for (const message of messages) for (const path of paths) {
    cases.push({prefix, errors: [{path, message}]});
    cases.push({prefix, errors: [{path, message}, {path: [], message: 'second'}, {path: ['x', '1'], message: ''}]});
  }
  cases.push({prefix, errors: Array.from({length: 105}, (_, i) => ({path: ['rows', String(i)], message: String(i)}))});
}
const formatCaseCount = cases.length;
for (const correlation_id of [undefined, null, '', 'fixture-id', '用户 😀']) {
  for (const errors of [undefined, null, [], [{path:['body','email'], message:'Required'}],
    [{path:['query','name'], message:'Public diagnostic'}, {path:[], message:'second\nline'}]]) {
    cases.push({request_response: true, correlation_id, errors});
  }
}
const requestCaseCount = cases.length-formatCaseCount;
for(const correlation_id of [undefined,null,'','fixture-id','用户 😀']) {
 for(const header_errors of [null,[],[{path:['x-result'],message:'Required'}]]) {
  for(const errors of [null,[],[{path:[],message:'Expected string, received number'}]]) cases.push({response_response:true,correlation_id,header_errors,errors});
 }
}
const native = spawnSync('cargo', ['run', '--quiet', '--manifest-path', path.join(root, 'framework-rs/Cargo.toml'), '-p', 'forklaunch-core', '--features', 'http', '--example', 'parse_errors_parity'], {
  input: cases.map(value => JSON.stringify(value)).join('\n') + '\n', encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, timeout: 300000,
  env: {...process.env, CARGO_INCREMENTAL: '0'},
});
assert.equal(native.status, 0, native.stderr);
const results = native.stdout.trim().split('\n').map(value => JSON.parse(value));
assert.equal(results.length, cases.length);
for (const [index, value] of cases.entries()) {
  if(value.response_response) {
    let count=0, observed=null;
    const req={contractDetails:{},context:{correlationId:value.correlation_id},schemaValidator:{
      union:values=>values,
      parse:()=>{const errors=count++===0?value.errors:value.header_errors;return errors===null?{ok:true}:{ok:false,errors};}
    }};
    const res={statusCode:200,bodyData:42,responseSchemas:{responses:{200:{}},headers:{}},
      getHeaders:()=>({}),type(type){observed={...observed,type};return this;},
      status(status){observed={...observed,status};return this;},send(body){observed={...observed,body};return this;}};
    parseResponse(req,res,()=>{});
    assert.deepEqual(results[index],observed?{...observed,type:'text/plain; charset=utf-8'}:null);
    continue;
  }
  if (!value.request_response) {
    assert.equal(results[index], prettyPrintParseErrors(value.errors, value.prefix), JSON.stringify(value));
    continue;
  }
  const observed = {};
  const req = {
    contractDetails:{}, context:{correlationId:value.correlation_id},
    schemaValidator:{parse:()=>({ok:false, errors:value.errors})},
    body:{}, params:{}, query:{}, headers:{}, requestSchema:{},
  };
  const res = {
    type(type){observed.type=type;return this;},
    status(status){observed.status=status;return this;},
    send(body){observed.body=body;return this;},
  };
  parseRequest(req,res,()=>assert.fail('invalid request reached handler'));
  // Actual request middleware declares plain text; Express adds its charset
  // when sending the string. The live canonical fixture verifies final media.
  assert.equal(observed.type,'text/plain');
  assert.deepEqual(results[index], {...observed, type:'text/plain; charset=utf-8'});
}
console.log(`PASS ${formatCaseCount} actual TypeScript parse-error formatting comparisons and ${requestCaseCount} actual request middleware error bodies/statuses (final media covered live)`);

console.log(`PASS ${cases.length-formatCaseCount-requestCaseCount} actual response middleware diagnostic envelopes including header/body ordering and successful no-error case`);

// Actual parse middleware re-enters when its diagnostic uses enriched send.
// Required application headers still fail; only that response's nested send
// bypasses validation. Subsequent independent responses must validate normally.
for(const invalid of [true,true,false]) {
 let parses=0,sends=0,delivered=0;
 const req={contractDetails:{},context:{correlationId:'reentrant'},schemaValidator:{
  union:values=>values,
  parse:()=>{parses++;return invalid && parses%2===0?{ok:false,errors:[{path:['x-result'],message:'Required'}]}:{ok:true};}
 }};
 const res={statusCode:200,bodyData:'ok',responseSchemas:{responses:{200:{}},headers:{}},getHeaders:()=>({}),
  type(){return this;},status(){return this;},send(body){
   assert.ok(++sends<=1,'diagnostic send must not recurse');
   assert.ok(body.includes('Header validation failed'));
   parseResponse(req,res,()=>{delivered++;});return this;
  }};
 parseResponse(req,res,()=>{delivered++;});
 assert.equal(parses,2);assert.equal(sends,invalid?1:0);assert.equal(delivered,1);
 assert.deepEqual(req.contractDetails,{},'validation must not mutate shared contract options');
}
console.log('PASS actual response middleware reentrancy regression: independent invalid responses and valid response');

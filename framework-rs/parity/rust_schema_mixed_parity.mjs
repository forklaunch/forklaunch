// Mixed resource drafts are checked natively before and after translation;
// independent AJV engines check each original resource and the combined output.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES ?? root,'package.json'));
const Ajv2020=require('ajv/dist/2020').default;
const drafts=[['04',require('ajv-draft-04').default],['06',require('ajv').default],['07',require('ajv').default],['2019-09',require('ajv/dist/2019').default],['2020-12',Ajv2020]];
const uri=draft=>draft.length===2?`http://json-schema.org/draft-${draft}/schema#`:`https://json-schema.org/draft/${draft}/schema`;
const fixtures=[];
for(const [draft,Validator] of drafts) {
 const resource={$schema:uri(draft),[draft==='04'?'id':'$id']:'https://mixed.fixture.test/child',type:'object',required:['value','tuple'],properties:{value:{type:'number',minimum:1,maximum:5,exclusiveMinimum:draft==='04'?true:1,exclusiveMaximum:draft==='04'?true:5},tuple:{type:'array',minItems:1,[draft==='2020-12'?'prefixItems':'items']:[{type:'string'}],[draft==='2020-12'?'items':'additionalItems']:{type:'integer'}},next:{$ref:'#'}},dependencies:{trigger:['extra']}};
 const cases=[[{value:2,tuple:['a',1]},true],[{value:2,tuple:['a'],next:{value:3,tuple:['b']}},true],[{value:1,tuple:['a']},false],[{value:5,tuple:['a']},false],[{value:2,tuple:[1]},false],[{value:2,tuple:['a','bad']},false],[{value:2,tuple:[]},false],[{value:2,tuple:['a'],trigger:true},false],[{value:2,tuple:['a'],trigger:true,extra:1},true]];
 const validator=new Validator({strict:false,allErrors:true});
 if(draft==='06')validator.addMetaSchema(require('ajv/dist/refs/json-schema-draft-06.json'));
 const independent=validator.compile(resource);
 for(const [value,valid] of cases)assert.equal(independent(value),valid);
 for(const [outer] of drafts) if(outer!==draft) fixtures.push({runtime:true,schema:{$schema:uri(outer),[outer==='04'?'id':'$id']:'https://mixed.fixture.test/root',type:'object',properties:{child:{$ref:'https://mixed.fixture.test/child'}},definitions:{Child:resource}},cases:cases.map(([child,valid])=>[{child},valid])});
}
// AJV marks all array items evaluated after contains; native validation only
// marks matching indices. Keep that pre-existing validator discrepancy explicit.
// Successful legacy applicators must contribute evaluated locations through refs.
for(const draft of ['06','07','2019-09']) {
 fixtures.push({runtime:true,schema:{$schema:uri('2020-12'),$id:'https://mixed.fixture.test/evaluated',type:'array',$ref:'child',unevaluatedItems:false,$defs:{Child:{$schema:uri(draft),$id:'child',contains:{type:'integer'}}}},cases:[[[1],true],[[1,'x'],false,true],[['x'],false]]});
 fixtures.push({runtime:true,schema:{$schema:uri('2020-12'),$id:'https://mixed.fixture.test/evaluated',type:'object',$ref:'child',unevaluatedProperties:false,$defs:{Child:{$schema:uri(draft),$id:'child',properties:{known:{type:'integer'}}}}},cases:[[{known:1},true],[{known:1,extra:1},false],[{known:'x'},false]]});
}
// Later keywords in older resources remain inactive annotations.
fixtures.push({runtime:true,schema:{$schema:uri('2020-12'),$id:'https://mixed.fixture.test/annotations',$ref:'child',$defs:{Child:{$schema:uri('04'),id:'child',type:'array',contains:false,prefixItems:[false],unevaluatedItems:false,items:{type:'string'}}}},cases:[[[],true],[['a'],true],[[1],false]]});
const bundled=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,asyncapi-import','--example','codegen_schema_bundle'],{input:JSON.stringify(fixtures),encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'},maxBuffer:16*1024*1024}));
let count=0, ajvDifferences=0;
for(const [i,fixture] of fixtures.entries()) {
 assert.equal(bundled[i].schema.$schema,uri('2020-12'));
 const compiled=new Ajv2020({strict:false,allErrors:true}).compile(bundled[i].schema);
 for(const [j,[value,valid,ajvValid=valid]] of fixture.cases.entries()) {
  assert.equal(bundled[i].original[j],valid,`native original ${i}/${j}`);
  assert.equal(bundled[i].compiled[j],valid,`native translated ${i}/${j}`);
  assert.equal(compiled(value),ajvValid,`AJV translated ${i}/${j}: ${JSON.stringify(compiled.errors)}`);
  count++;if(ajvValid!==valid)ajvDifferences++;
 }
}
console.log(`PASS ${count} mixed-draft original/translated native cases across ${fixtures.length} fixtures; ${count-ajvDifferences} AJV agreements and ${ajvDifferences} recorded contains/unevaluatedItems differences`);

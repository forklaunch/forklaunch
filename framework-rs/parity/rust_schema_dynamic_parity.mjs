// Compare the actual SDK graph compiler with independent AJV 2019-09/2020-12 evaluation.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES ?? root,'package.json'));
const Ajv=require('ajv/dist/2020').default;
const Ajv2019=require('ajv/dist/2019').default;
const AjvLegacy=require('ajv').default;
const AjvDraft4=require('ajv-draft-04').default;
const node=(id,type)=>({$id:id,$dynamicAnchor:'node',type:'object',additionalProperties:false,required:['value'],properties:{value:{type},next:{$ref:'base'}}});
const schema={
 $schema:'https://json-schema.org/draft/2020-12/schema',$id:'https://schema.fixture.test/root',type:'object',additionalProperties:false,
 properties:{left:{$ref:'a'},right:{$ref:'b'}},$defs:{
  A:node('a','string'),B:node('b','integer'),
  Base:{$id:'base',$dynamicAnchor:'node',type:'object',additionalProperties:false,properties:{child:{$dynamicRef:'#node'},static:{$ref:'#node'}}}
 }
};
const fixtures=[
 {schema,cases:[
  [{},true],[{left:{value:'a'}},true],[{left:{value:1}},false],
  [{right:{value:1}},true],[{right:{value:'a'}},false],
  [{left:{value:'a',next:{child:{value:'b'}}}},true],
  [{left:{value:'a',next:{child:{value:1}}}},false],
  [{right:{value:1,next:{child:{value:2}}}},true],
  [{right:{value:1,next:{child:{value:'b'}}}},false],
  [{left:{value:'a',next:{static:{}}}},true],
  [{left:{value:'a',next:{static:{value:'b'}}}},false],
  [{left:{value:'a',next:{static:{child:{value:'b'}}}}},true],
  // AJV currently leaks the first sibling's dynamic binding into the second.
  [{left:{value:'a'},right:{value:1,next:{child:{value:2,next:{child:{value:3}}}}}},true,false],
 ]},
 {schema:{$schema:'https://json-schema.org/draft/2020-12/schema',$id:'https://schema.fixture.test/static-root',$dynamicAnchor:'node',$ref:'target#node',$defs:{Target:{$id:'target',$dynamicAnchor:'node',type:'string'}}},cases:[['value',true],[1,false]]},
 {ajvUnsupported:true,schema:{$schema:'https://json-schema.org/draft/2020-12/schema',$id:'https://schema.fixture.test/pointer-root',$dynamicAnchor:'node',$dynamicRef:'target#/$defs/Value',$defs:{Target:{$id:'target',$dynamicAnchor:'node',$defs:{Value:{type:'integer'}}}}},cases:[[1,true],['value',false]]},
 {schema:{$schema:'https://json-schema.org/draft/2020-12/schema',$id:'https://schema.fixture.test/plain-root',$dynamicAnchor:'node',type:'object',properties:{next:{$dynamicRef:'#plain'}},$defs:{Value:{$anchor:'plain',type:'string'}}},cases:[[{next:'value'},true,false],[{next:{}},false,true]]},
 {ajvUnsupported:true,schema:{$schema:'https://json-schema.org/draft/2020-12/schema',$id:'https://schema.fixture.test/uri-root',$dynamicAnchor:'node',type:'object',properties:{next:{$dynamicRef:'target#node'}},$defs:{Target:{$id:'target',$dynamicAnchor:'node',type:'string'}}},cases:[[{next:{}},true],[{next:'value'},false]]},
];
// The 2019-09 predecessor uses a single resource-root recursive binding.
const recursiveSchema=JSON.parse(JSON.stringify(schema).replaceAll('2020-12','2019-09').replaceAll('"$dynamicAnchor":"node"','"$recursiveAnchor":true').replaceAll('"$dynamicRef":"#node"','"$recursiveRef":"#"').replaceAll('"$ref":"#node"','"$ref":"#"'));
fixtures.push({schema:recursiveSchema,cases:fixtures[0].cases});
fixtures.push({schema:{$schema:'https://json-schema.org/draft/2019-09/schema',$recursiveAnchor:true,type:'object',additionalProperties:false,properties:{next:{$ref:'#/$defs/Base'}},$defs:{Base:{$id:'https://schema.fixture.test/nonrecursive',$recursiveAnchor:false,type:'object',additionalProperties:false,properties:{child:{$recursiveRef:'#'}}}}},cases:[[{next:{child:{}}},true],[{next:{child:{next:{}}}},false,true]]});
for(const [draft,id] of [['04','id'],['06','$id'],['07','$id']]) {
 fixtures.push({schema:{$schema:`http://json-schema.org/draft-${draft}/schema#`,[id]:'https://schema.fixture.test/sibling/',type:'object',properties:{value:{[id]:'other/',$ref:'real'}},definitions:{First:{[id]:'real',type:'string'},Second:{[id]:'other/real',type:'integer'}}},cases:[[{value:'text'},false],[{value:1},true]]});
 fixtures.push({schema:{$schema:`http://json-schema.org/draft-${draft}/schema#`,[id]:'https://schema.fixture.test/legacy',type:'object',properties:{first:{$ref:'#Node:v1'},second:{$ref:'other#Node:v1'}},definitions:{
  Node:{[id]:'#Node:v1',type:'object',required:['label'],properties:{label:{type:'string'},next:{$ref:'#Node:v1'}}},
  Other:{[id]:'other',definitions:{Value:{[id]:'#Node:v1',type:'integer'}}}
 },examples:[{[id]:'#Node:v1'}]},cases:[
  [{},true],[{first:{label:'a',next:{label:'b'}},second:1},true],
  [{first:{label:1}},false],[{first:{label:'a',next:{label:1}}},false],[{second:'value'},false]
 ]});
}
fixtures.push(
 {schema:{$schema:'http://json-schema.org/draft-07/schema#',$id:'https://schema.fixture.test/dependencies',type:'object',dependencies:{trigger:{$ref:'rule'},other:['required']},definitions:{Rule:{$id:'rule',required:['value'],properties:{value:{type:'integer'}}}}},cases:[ [{},true],[{trigger:true,value:1},true],[{trigger:true},false],[{trigger:true,value:'bad'},false],[{other:1},false],[{other:1,required:2},true] ]},
 {schema:{$schema:'http://json-schema.org/draft-07/schema#',$id:'https://schema.fixture.test/tuple',type:'array',items:[{$id:'#Word',type:'string'},{$ref:'#/definitions/Count'}],additionalItems:{$ref:'#Word'},definitions:{Count:{type:'integer'}}},cases:[ [[],true],[['a',1,'b'],true],[['a','bad'],false],[['a',1,false],false] ]},
 {schema:{$schema:'https://json-schema.org/draft/2020-12/schema',$id:'https://schema.fixture.test/unevaluated-object',type:'object',properties:{known:true},unevaluatedProperties:{$ref:'word'},$defs:{Word:{$id:'word',type:'string'}}},cases:[ [{known:1,extra:'a'},true],[{known:1,extra:1},false],[{known:1},true] ]},
 {schema:{$schema:'https://json-schema.org/draft/2020-12/schema',$id:'https://schema.fixture.test/unevaluated-array',type:'array',prefixItems:[{type:'boolean'}],unevaluatedItems:{$ref:'#/$defs/Word'},$defs:{Word:{type:'string'}}},cases:[ [[true,'a'],true],[[true,1],false],[[false],true] ]},
 {schema:{$schema:'https://json-schema.org/draft/2019-09/schema',$id:'https://schema.fixture.test/content',type:'object',properties:{parsed:{$ref:'#/contentSchema'}},contentSchema:{$ref:'#/$defs/Word'},$defs:{Word:{type:'string'}}},cases:[ [{parsed:'value'},true],[{parsed:1},false] ]}
);
for(const dialect of ['http://json-schema.org/draft-04/schema#','http://json-schema.org/draft-06/schema#','http://json-schema.org/draft-07/schema#','https://json-schema.org/draft/2019-09/schema','https://json-schema.org/draft/2020-12/schema']) {
 fixtures.push({schema:{$schema:dialect,$ref:'#/definitions/Word',minLength:2,maxLength:4,definitions:{Word:{type:'string',pattern:'^[a-z]+$'}}},cases:[['ab',true],['a',false],['abcde',false],['12',false],[42,false]]});
}
fixtures.push({schema:{$schema:'https://json-schema.org/draft/2020-12/schema',$ref:'#/$defs/Base',properties:{extra:{type:'integer'}},unevaluatedProperties:false,$defs:{Base:{type:'object',required:['base'],properties:{base:{type:'string'}}}}},cases:[[{base:'a'},true],[{base:'a',extra:1},true],[{base:'a',unknown:1},false],[{base:'a',extra:'bad'},false],[{},false]]});
fixtures.push({schema:{$schema:'https://json-schema.org/draft/2020-12/schema',$ref:'#/$defs/Base',allOf:[{minLength:2}],maxLength:4,$defs:{Base:{type:'string'}}},cases:[['ab',true],['a',false],['abcde',false]]});
// AJV default mode shadows its dynamic-reference validity flag and skips the static sibling; allErrors mode evaluates both.
fixtures.push({allErrorsParity:true,schema:{$schema:'https://json-schema.org/draft/2020-12/schema',$dynamicAnchor:'node',type:'object',required:['value'],properties:{value:{type:'integer'},child:{$ref:'#/$defs/Base',$dynamicRef:'#node'}},$defs:{Base:{type:'object',properties:{value:{minimum:1}}}}},cases:[[{value:0},true],[{value:0,child:{value:1}},true],[{value:0,child:{value:0}},false,true],[{value:0,child:{value:'1'}},false],[{value:0,child:{value:1,child:{value:2}}},true],[{value:0,child:{value:1,child:{value:0}}},false,true]]});
fixtures.push({allErrorsParity:true,schema:{$schema:'https://json-schema.org/draft/2019-09/schema',$recursiveAnchor:true,type:'object',required:['value'],properties:{value:{type:'integer'},child:{$ref:'#/$defs/Base',$recursiveRef:'#'}},$defs:{Base:{type:'object',properties:{value:{minimum:1}}}}},cases:[[{value:0},true],[{value:0,child:{value:1}},true],[{value:0,child:{value:0}},false,true],[{value:0,child:{value:'1'}},false],[{value:0,child:{value:1,child:{value:2}}},true],[{value:0,child:{value:1,child:{value:0}}},false,true]]});
for(const [draft,id] of [['04','id'],['06','$id'],['07','$id']]) {
 fixtures.push({schema:{$schema:`http://json-schema.org/draft-${draft}/schema#`,[id]:'https://schema.fixture.test/combined-root',type:'object',properties:{word:{$ref:'other#Word'},count:{$ref:'other#Count'},leaf:{$ref:'other#Leaf'}},definitions:{Word:{[id]:'other#Word',type:'object',required:['value'],properties:{value:{type:'string'},next:{$ref:'#Word'},leaf:{$ref:'#Leaf'}},definitions:{Leaf:{[id]:'#Leaf',type:'integer'}}},Count:{[id]:'other#Count',type:'integer'}}},cases:[ [{word:{value:'a',next:{value:'b'}},count:1},true],[{word:{value:1}},false],[{word:{value:'a',next:{value:2}}},false],[{count:'bad'},false],[{word:{value:'a',leaf:1}},true],[{word:{value:'a',leaf:'bad'}},false],[{leaf:1},true],[{leaf:'bad'},false] ]});
}
for(const [draft,id] of [['04','id'],['06','$id'],['07','$id']]) {
 for(const [identity,reference] of [['#1name','#1name'],['#_name','#_name'],['#/alias','#/alias'],['other#/alias','other#/alias'],['#%41','#A'],['#%2Falias','#%2Falias'],['#%E6%B0%B4','#%E6%B0%B4'],['#a%20b','#a%20b'],['other%41#Word','otherA#Word'],['other?x=%41#Word','other?x=A#Word'],['other%2fpart#Word','other%2Fpart#Word']]) {
  fixtures.push({schema:{$schema:`http://json-schema.org/draft-${draft}/schema#`,[id]:'https://schema.fixture.test/permissive',type:'object',properties:{value:{$ref:reference}},definitions:{Value:{[id]:identity,type:'string'}}},cases:[[{value:'a'},true],[{value:1},false]]});
 }
 fixtures.push({schema:{$schema:`http://json-schema.org/draft-${draft}/schema#`,[id]:'https://schema.fixture.test/encoded-aliases',type:'object',alias:{type:'boolean'},properties:{encoded:{$ref:'#%2Falias'},literal:{$ref:'#/alias'}},definitions:{Encoded:{[id]:'#%2Falias',type:'string'},Literal:{[id]:'#/alias',type:'integer'}}},cases:[[{encoded:'a',literal:1},true],[{encoded:1},false],[{literal:'a'},false],[{literal:true},false]]});
 fixtures.push({schema:{$schema:`http://json-schema.org/draft-${draft}/schema#`,[id]:'https://schema.fixture.test/alias-sibling',type:'object',properties:{value:{$ref:'other#Alias'}},definitions:{Alias:{[id]:'other#Alias',$ref:'target'},Target:{[id]:'target',type:'string'}}},cases:[[{value:'a'},true],[{value:1},false]]});
}
for(const dialect of ['2019-09','2020-12']) for(const [identity,reference] of [['other%41','otherA#Word'],['other?x=%41','other?x=A#Word'],['other%2fpart','other%2Fpart#Word']]) {
 fixtures.push({schema:{$schema:`https://json-schema.org/draft/${dialect}/schema`,$id:'https://schema.fixture.test/normalized',type:'object',properties:{value:{$ref:reference}},$defs:{Word:{$id:identity,$anchor:'Word',type:'string'}}},cases:[[{value:'a'},true],[{value:1},false]]});
}
const bundled=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,asyncapi-import','--example','codegen_schema_bundle'],{input:JSON.stringify(fixtures),encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'}}));
let count=0, differences=0, unsupported=0, allErrorsComparisons=0;
for(let i=0;i<fixtures.length;i++) {
 let original;
 const dialect=fixtures[i].schema.$schema;
 const Validator=dialect.includes('2019-09')?Ajv2019:dialect.includes('2020-12')?Ajv:dialect.includes('draft-04')?AjvDraft4:AjvLegacy;
 const validator=(allErrors=false)=>{const value=new Validator({strict:false,allErrors});if(dialect.includes('draft-06'))value.addMetaSchema(require('ajv/dist/refs/json-schema-draft-06.json'));return value;};
 if(fixtures[i].ajvUnsupported) {
  assert.throws(()=>validator().compile(fixtures[i].schema),/only supports hash fragment reference/);
  unsupported+=fixtures[i].cases.length;
 } else { original=validator().compile(fixtures[i].schema); }
 const allErrorsOriginal=fixtures[i].allErrorsParity ? validator(true).compile(fixtures[i].schema) : undefined;
 const compiled=validator().compile(bundled[i].schema);
 for(const [index,[value,valid,ajvValid=valid]]of fixtures[i].cases.entries()) {
  assert.equal(bundled[i].original[index],valid,`Native original ${i}/${index}`);
  if(allErrorsOriginal) {assert.equal(allErrorsOriginal(value),valid,`AJV allErrors original ${i}/${index}`);allErrorsComparisons++;}
  assert.equal(bundled[i].compiled[index],valid,`Native compiled ${i}/${index}`);
  if(original) assert.equal(original(value),ajvValid,`AJV fixture ${i}: ${JSON.stringify(value)} ${JSON.stringify(original.errors)}`);
  assert.equal(compiled(value),valid,`SDK graph fixture ${i}: ${JSON.stringify(value)} ${JSON.stringify(compiled.errors)}`);
  count++; if(valid!==ajvValid) differences++;
 }
}
console.log(`PASS ${count} native/SDK dynamic-scope cases, ${count-differences-unsupported} independent AJV agreements, ${differences} recorded AJV behavior differences and ${unsupported} AJV-unsupported URI cases`);

if(allErrorsComparisons) console.log(`PASS ${allErrorsComparisons} independent AJV allErrors comparisons for composed static/dynamic references`);

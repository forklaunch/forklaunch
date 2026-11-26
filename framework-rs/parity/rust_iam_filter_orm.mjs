// Compare bounded native predicates with actual configured IAM entity metadata,
// MikroORM and PostgreSQL. No in-memory FilterQuery matcher is substituted.
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {betterAuthOrm} from './fixtures/rust_better_auth_orm.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const source=await betterAuthOrm(root,process.env.FORKLAUNCH_TEST_ORM_DEPENDENCIES,undefined,true,'blueprint/iam-base/persistence/entities/index.ts');
const schema=`fl_iam_filter_${crypto.randomBytes(8).toString('hex')}`;
const orm=await source.MikroORM.init({entities:Object.values(source.entities),clientUrl:process.env.FORKLAUNCH_TEST_POSTGRES_URL,schema});
const id=index=>`a0000000-0000-4000-8000-${String(index).padStart(12,'0')}`;
const dates={createdAt:'2026-01-01T00:00:00.000Z',updatedAt:'2026-01-01T00:00:00.000Z'};
const organizations=[{id:id(1),name:'Org',domain:'one.test',subscription:'one',status:'active',...dates},{id:id(2),name:'Other',domain:'two.test',subscription:'two',status:'active',logoUrl:null,...dates}];
const permissions=[{id:id(3),slug:'platform:read',...dates},{id:id(4),slug:'other:read',...dates}];
const roles=[{id:id(5),name:'Reader',permissions:[id(3)],...dates},{id:id(6),name:'Other',permissions:[id(4)],...dates}];
const users=[
 {id:id(7),email:'Case@example.test',firstName:'Alpha',lastName:'Member',phoneNumber:null,roles:[id(5)],organization:id(1),subscription:null,providerFields:null,...dates},
 {id:id(8),email:'case@example.test',firstName:'Beta',lastName:'Member',phoneNumber:'123',roles:[id(5),id(6)],organization:id(1),subscription:id(11),providerFields:null,...dates},
 {id:id(9),email:'other@example.test',firstName:'Gamma',lastName:'Else',roles:[id(6)],organization:id(2),providerFields:null,...dates},
 {id:id(10),email:'none@example.test',firstName:'Delta',lastName:'Else',phoneNumber:'',roles:[],organization:null,providerFields:null,...dates},
];
const cmp=(field,test)=>({compare:{field,test}}),related=(field,ids)=>({related:{field,ids}});
const cases=[];
function add(name,filter,extra={}){cases.push({name,kind:'user',filter,...extra});}
add('empty AND', {all:[]});
add('empty OR', {any:[]});
add('NOT empty AND', {not:{all:[]}});
add('NOT empty OR', {not:{any:[]}});
add('empty AND scoped', {all:[]}, {scope:id(1)});
add('NOT empty OR scoped', {not:{any:[]}}, {scope:id(1)});
add('AND empty OR', {all:[{any:[]},cmp('firstName',{eq:'Alpha'})]});
add('OR empty AND', {any:[{all:[]},cmp('firstName',{eq:'Alpha'})]});
add('nested empty identities', {all:[{all:[]},{any:[{any:[]},cmp('firstName',{eq:'Alpha'})]}]});
add('case-sensitive email',cmp('email',{eq:'Case@example.test'}));
add('case-sensitive email negation',cmp('email',{ne:'Case@example.test'}));
add('nullable ne is unknown',cmp('phoneNumber',{ne:'123'}));
add('not-eq excludes null',{not:cmp('phoneNumber',{eq:'123'})});
add('explicit null includes missing',cmp('phoneNumber','isNull'));
add('not-null excludes missing',cmp('phoneNumber','isNotNull'));
add('not explicit null',{not:cmp('phoneNumber','isNull')});
add('empty in',cmp('phoneNumber',{in:[]}));
add('empty not-in',cmp('phoneNumber',{notIn:[]}));
add('in duplicates',cmp('firstName',{in:['Alpha','Alpha','Gamma']}));
add('not-in nullable',cmp('phoneNumber',{notIn:['123']}));
add('AND three-value',{all:[cmp('lastName',{eq:'Member'}),{not:cmp('phoneNumber',{eq:'123'})}]});
add('OR three-value',{any:[cmp('phoneNumber',{eq:'123'}),cmp('firstName',{eq:'Alpha'})]});
add('nested NOT OR',{not:{any:[cmp('phoneNumber',{eq:'123'}),cmp('firstName',{eq:'Alpha'})]}});
add('UUID-looking text stays exact',cmp('subscription',{eq:id(11).toUpperCase()}));
add('uppercase UUID',cmp('id',{eq:id(7).toUpperCase()}));
add('uppercase FK',cmp('organizationId',{eq:id(1).toUpperCase()}));
add('nullable FK',cmp('organizationId','isNull'));
add('missing related UUID',cmp('organizationId',{eq:id(999)}));
add('roles some',related('roleIds',[id(5)]));
add('roles empty',related('roleIds',[]));
add('roles uppercase',related('roleIds',[id(5).toUpperCase()]));
add('roles none',{not:related('roleIds',[id(5)])});
add('roles missing',related('roleIds',[id(999)]));
add('scope outside OR',{any:[cmp('firstName',{eq:'Alpha'}),cmp('id',{eq:id(9)})]},{scope:id(1)});
add('scope outside NOT',{not:cmp('firstName',{eq:'Beta'})},{scope:id(1)});
add('selector order and duplicates',cmp('email','isNotNull'),{ids:[id(9),id(7),id(7)]});
add('uppercase selectors',cmp('email','isNotNull'),{ids:[id(7).toUpperCase()]});
add('empty selector',cmp('email','isNotNull'),{ids:[]});
add('permission relation',related('permissionIds',[id(3)]),{kind:'role'});
add('permission literal',cmp('slug',{eq:'platform:read'}),{kind:'permission'});
add('organization null',cmp('logoUrl','isNull'),{kind:'organization'});
add('organization status',cmp('status',{eq:'active'}),{kind:'organization'});
// Exhaust every unary/binary logical shape through depth two over empty
// groups plus concrete true/false/NULL-producing predicates. PostgreSQL is
// the oracle for omitted SQL fragments, including nested OR and negation.
const atoms=[{all:[]},{any:[]},cmp('firstName',{eq:'Alpha'}),cmp('phoneNumber',{eq:'123'})];
const firstLevel=[...atoms,...atoms.map(not=>({not}))];
for(const all of atoms)for(const right of atoms)for(const op of ['all','any'])firstLevel.push({[op]:[all,right]});
let logicalIndex=0;
for(const child of firstLevel)add(`logical unary ${logicalIndex++}`,{not:child});
for(const left of firstLevel)for(const right of firstLevel)for(const op of ['all','any'])add(`logical binary ${logicalIndex++}`,{[op]:[left,right]});
const fields={organizationId:'organization',roleIds:'roles',permissionIds:'permissions'};
function query(filter){
 if(filter.all)return {$and:filter.all.map(query)};
 if(filter.any)return {$or:filter.any.map(query)};
 if(filter.not)return {$not:query(filter.not)};
 if(filter.related)return {[fields[filter.related.field]]:{$some:{id:{$in:filter.related.ids}}}};
 const {field,test}=filter.compare,key=fields[field]??field;
 if(test==='isNull')return {[key]:{$eq:null}};
 if(test==='isNotNull')return {[key]:{$ne:null}};
 const [op,value]=Object.entries(test)[0];return {[key]:{[`$${op==='notIn'?'nin':op}`]:value}};
}
const entities={user:source.entities.User,role:source.entities.Role,permission:source.entities.Permission,organization:source.entities.Organization};
try{
 await orm.schema.create();const em=orm.em.fork();
 for(const [kind,rows] of [['organization',organizations],['permission',permissions],['role',roles],['user',users]]){
  for(const row of rows){const value={...row,createdAt:new Date(row.createdAt),updatedAt:new Date(row.updatedAt)};em.persist(em.create(entities[kind],value));}
  await em.flush();
 }
 const expected=[];
 for(const item of cases){
  const clauses=[query(item.filter)];if(item.scope)clauses.push({organization:item.scope});if(item.ids)clauses.push({id:{$in:item.ids}});
  const rows=await orm.em.fork().find(entities[item.kind],{$and:clauses});expected.push({ids:rows.map(row=>row.id).sort()});
 }
 // Check the underlying ORM identities directly as well as native comparisons.
 const emptyGroups={};
 for(const key of ['$and','$or'])emptyGroups[key]=(await orm.em.fork().find(source.entities.User,{[key]:[]})).length;
 assert.deepEqual(emptyGroups,{'$and':4,'$or':0},'actual ORM empty AND matches all and empty OR matches none');
 await assert.rejects(()=>orm.em.fork().find(source.entities.User,{id:'not-a-uuid'}));
 const input={organizations,permissions,roles,users,cases};
 if(process.env.FORKLAUNCH_TEST_IAM_FILTER_CAPTURE)writeFileSync(process.env.FORKLAUNCH_TEST_IAM_FILTER_CAPTURE,JSON.stringify({input,expected}));
 if(process.env.FORKLAUNCH_TEST_IAM_FILTER_ORACLE_ONLY!=='1'){
  const native=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'blueprint-rs/Cargo.toml'),'-p','forklaunch-iam-base','--example','directory_filter_parity'],{input:JSON.stringify(input),encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'}}));
  assert.equal(native.length,expected.length);
  assert.deepEqual(native[cases.findIndex(item=>item.name==='selector order and duplicates')].ids,[id(7),id(9)],'native insertion sequence retained independent of selector order');
  for(let index=0;index<native.length;index++)assert.deepEqual({...native[index],ids:native[index].ids?.sort()},expected[index],cases[index].name);
 }
 console.log(`PASS actual MikroORM/PostgreSQL IAM bounded filters: ${cases.length} cases, NULL/NOT/membership, UUID-only normalization, related IDs, mandatory scope and duplicate selectors`);
}finally{await orm.schema.drop({dropDb:false,dropMigrationsTable:true});await orm.close(true);}

// Actual IAM services and application mappers; this fixture owns only ORM storage.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
class Collection {
 constructor(values=[]){this.values=[...values];}
 isInitialized(){return true;}
 async init(){return this;}
 getItems(){return this.values;}
 add(value){if(!this.values.some(item=>item.id===value.id))this.values.push(value);}
 remove(value){this.values=this.values.filter(item=>item.id!==value.id);}
 contains(value){return this.values.some(item=>item.id===value.id);}
}
const entities=new Map(),rows=new Map(),aliases=new Map();
const families=['Permission','Role','User','Organization'];
for(const name of families){entities.set(name,{name});rows.set(name,new Map());}
function pojo(value){
 if(value instanceof Collection)return value.getItems().map(value=>({id:value.id}));
 if(value instanceof Date)return value;
 return value;
}
const captured=[];
const handlers=new Proxy({}, {get:(_,method)=>(_validator,_path,options)=>({method,options})});
function router(prefix) {return new Proxy({}, {get:(_,method)=>(suffix,handler)=>{assert.equal(method,handler.method);captured.push({method:method.toUpperCase(),path:(prefix+suffix).replace(/\/$/,''),options:handler.options});return handler;}});}
// Controllers bind runtime credentials eagerly (bindRuntimeAuth validates they
// are non-empty strings); key and URL tokens resolve to fixture strings.
const ci={resolve:token=>/(_KEY|_URL)$/.test(token)?`fixture-${token}`:{},scopedResolver:()=>()=>({})},tokens=new Proxy({}, {get:(_,key)=>key});
let currentAction;
let actual;
actual=actualValidator(root,(spec,file,load)=>{
 if(spec.endsWith('/bootstrapper'))return {ci,tokens};
 if(spec==='jose')return {};
 if(spec.endsWith('/persistence/encryptedType'))return {withEncryptionContext:(_context,run)=>run()};
 // Contracts import a concrete validator that the CLI rewrites to the app's
 // choice; this parity run selects Zod for every layer.
 if(spec==='@forklaunch/validator/zod'||spec==='@forklaunch/validator/typebox')return actual.statics;
 if(spec==='@forklaunch/blueprint-core')return {...actual.statics,schemaValidator:actual.validator,handlers,forklaunchRouter:router,IdSchema:{id:actual.statics.uuid},IdsSchema:{ids:actual.statics.array(actual.statics.uuid)},...load(path.join(root,'blueprint/core/auth/rbac.ts')),...load(path.join(root,'blueprint/core/auth/bindRuntimeAuth.ts'))};
 if(spec==='@forklaunch/core/mappers')return {...load(path.join(root,'framework/core/src/mappers/mapper.ts')),...load(path.join(root,'framework/core/src/mappers/mapServiceSchemas.ts'))};
 if(spec==='@forklaunch/internal')return load(path.join(root,'framework/internal/src/serviceSchemaResolver.ts'));
 if(spec==='@forklaunch/implementation-iam-base/schemas')return load(path.join(root,'blueprint/implementations/iam/base/domain/schemas/index.ts'));
 if(spec.startsWith('./typebox/'))return {};
 if(spec.includes('/persistence/entities')){
  if(spec.endsWith('/entities'))return Object.fromEntries(entities);
  const name=path.basename(spec,'.entity'),capital=name[0].toUpperCase()+name.slice(1);return {[capital]:entities.get(capital)};
 }
 if(spec==='@mikro-orm/core')return {wrap:entity=>({toPOJO:()=>Object.fromEntries(Object.entries(entity).map(([key,value])=>[key,key==='organization'&&value?{id:value.id}:pojo(value)]))})};
 if(spec==='@forklaunch/core/http')return {evaluateTelemetryOptions:()=>({enabled:{}})};
});
function matches(row,filter){
 if(typeof filter==='string')return row.id===filter;
 return Object.entries(filter??{}).every(([key,value])=>{
  assert.notEqual(key,'ids','DTO ids must be converted to an ORM id filter');
  const stored=key==='organization'?row.organization?.id:row[key];
  if(value&&typeof value==='object'&&'$in' in value)return value.$in.includes(stored);
  if(key==='organization'&&value&&typeof value==='object')return stored===value.id;
  return stored===value;
 });
}
const em={
 create(entity,fields){
  const row={id:randomUUID(),...fields};
  for(const field of ['permissions','roles','users'])if(Array.isArray(row[field]))row[field]=new Collection(row[field]);
  if(entity.name==='User'&&row.organization)row.organization.users.add(row);
  rows.get(entity.name).set(row.id,row);return row;
 },
 async find(entity,filter){if(currentAction?.method==='updateBatchPermissions')assert.notEqual(this,em,'batch permission reads must use the transaction manager');return [...rows.get(entity.name).values()].filter(row=>matches(row,filter));},
 async findOne(entity,filter){return (await this.find(entity,filter))[0]??null;},
 async findOneOrFail(entity,filter){const row=await this.findOne(entity,filter);assert.ok(row,'entity not found');return row;},
 assign(entity,fields){for(const [key,value]of Object.entries(fields))entity[key]=['roles','permissions','users'].includes(key)&&Array.isArray(value)?new Collection(value):value;},
 persist(){if(currentAction?.method==='updateBatchPermissions')assert.notEqual(this,em,'batch permission writes must use the transaction manager');return {flush:async()=>{}};},
 merge(entity){return entity;},
 async upsertMany(values){return values;},
 async transactional(run){return run(Object.create(this));},
 async nativeDelete(entity,filter){
  for(const row of await this.find(entity,filter)){
   rows.get(entity.name).delete(row.id);
   for(const user of rows.get('User').values()){
    if(entity.name==='Role')user.roles.remove(row);
    if(entity.name==='Organization'&&user.organization?.id===row.id)user.organization=null;
   }
   if(entity.name==='Permission')for(const role of rows.get('Role').values())role.permissions.remove(row);
   if(entity.name==='User')row.organization?.users.remove(row);
  }
 }
};
const collector=new Proxy({}, {get:()=>()=>{}}),services={};
for(const name of ['Role','User','Organization','Permission']){
 const file=path.join(root,`blueprint/implementations/iam/base/services/${name.toLowerCase()}.service.ts`);
 const Constructor=actual.load(file)[`Base${name}Service`];
 const mappers=actual.load(path.join(root,`blueprint/iam-base/domain/mappers/${name.toLowerCase()}.mappers.ts`));
 services[name]=name==='Permission'?new Constructor(em,()=>services.Role,collector,actual.validator,{...mappers,RoleEntityMapper:actual.load(path.join(root,'blueprint/iam-base/domain/mappers/role.mappers.ts')).RoleEntityMapper}):name==='User'?new Constructor(em,()=>services.Role,()=>services.Organization,collector,actual.validator,mappers):new Constructor(em,collector,actual.validator,mappers);
}
function resolve(value){
 if(typeof value==='string'&&value.startsWith('@'))return aliases.get(value.slice(1))??value;
 if(Array.isArray(value))return value.map(resolve);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,value])=>[key,resolve(value)]));
 return value;
}
function normalize(value){
 if(typeof value==='string'){for(const[label,id]of aliases)if(value===id)return '@'+label;return value;}
 if(Array.isArray(value))return value.map(normalize);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).filter(([key,value])=>!['createdAt','updatedAt'].includes(key)&&value!==undefined).map(([key,value])=>[key,normalize(value)]));
 return value??null;
}
const actions=[];
function action(service,method,input={},options={}){actions.push({service,method,input,...options});}
const ref=label=>'@'+label;
action('Permission','createPermission',{slug:'platform.read',providerFields:{ignored:true}},{label:'read'});
action('Role','createRole',{name:'Readers',permissionIds:[ref('read')]},{label:'reader'});
action('Role','createRole',{name:'Editors',permissionIds:[ref('read')]},{label:'editor'});
action('Role','createBatchRoles',[{name:'Batch one',permissionIds:[ref('read')]},{name:'Batch two',providerFields:{ignored:true}}],{labels:['batchRoleOne','batchRoleTwo']});
action('Role','updateBatchRoles',[{id:ref('batchRoleOne'),name:'First change'},{id:ref('batchRoleOne'),name:'Final change'},{id:ref('batchRoleTwo'),permissionIds:[ref('read'),'00000000-0000-4000-8000-000000000000']}]);
action('Permission','createPermission',{slug:'platform.write',addToRolesIds:[ref('editor')]},{label:'write'});
action('Permission','createBatchPermissions',[{slug:'platform.admin',addToRolesIds:[ref('reader'),ref('editor')]},{slug:'platform.audit'}],{labels:['admin','audit']});
action('Role','getBatchRoles',{ids:[ref('editor'),ref('reader'),ref('editor'),'00000000-0000-4000-8000-000000000000']});
action('Permission','updatePermission',{id:ref('write'),slug:'platform.edit',addToRolesIds:[ref('reader')],removeFromRolesIds:[ref('editor')],providerFields:{ignored:true}});
action('Permission','updateBatchPermissions',[{id:ref('read'),slug:'platform.view',addToRolesIds:[ref('editor')]},{id:ref('audit'),slug:'platform.inspect',addToRolesIds:[ref('reader')],removeFromRolesIds:[ref('editor')]}]);
action('Role','getRole',{id:ref('reader')});
action('Role','updateRole',{id:ref('editor'),name:'Auditors',permissionIds:[ref('audit')]});
action('Organization','createOrganization',{name:'Tenant organization',domain:'fixture.test',subscription:'org-sub',logoUrl:'',providerFields:{enabled:true}},{label:'org'});
action('User','createUser',{email:'member@fixture.test',password:'unused-crud-password',firstName:'Member',lastName:'One',organization:ref('org'),roles:[ref('reader'),ref('editor')],phoneNumber:'+15550000001',subscription:'user-sub',providerFields:{opaque:1}},{label:'member'});
action('User','createBatchUsers',[{email:'second@fixture.test',password:'unused',firstName:'Second',lastName:'Member',roles:[ref('reader')]},{email:'third@fixture.test',password:'unused',firstName:'Third',lastName:'Member',roles:[]}],{labels:['second','third']});
action('User','getUser',{id:ref('member')});
action('User','getOrganizationIdByUserId',{id:ref('member')});
action('User','getOrganizationIdByUserId',{id:'00000000-0000-4000-8000-000000000000'});
action('User','surfaceRoles',{id:ref('member')});
action('User','surfacePermissions',{id:ref('member')});
action('User','updateUser',{id:ref('member'),password:'still-unused',roles:[ref('editor')],firstName:'Updated'});
action('User','updateBatchUsers',[{id:ref('second'),firstName:'Changed'},{id:ref('third'),lastName:'Changed'}]);
action('User','updateBatchUsers',[{id:ref('second'),firstName:'First change'},{id:ref('second'),firstName:'Final change'}]);
action('User','getBatchUsers',{ids:[ref('member'),ref('second'),ref('third')],organization:ref('org')});
action('Organization','getOrganization',{id:ref('org')});
action('Organization','updateOrganization',{id:ref('org'),logoUrl:'https://fixture.test/logo',name:'Changed organization',providerFields:null});
action('Permission','getBatchPermissions',{ids:[ref('read'),ref('audit')]});
action('User','deleteUser',{id:ref('member'),organization:'00000000-0000-4000-8000-000000000000'});
action('User','getUser',{id:ref('member')});
action('Role','deleteRole',{id:ref('editor')});
action('User','getUser',{id:ref('member')});
action('Permission','deleteBatchPermissions',{ids:[ref('write'),ref('admin')]});
action('Role','getRole',{id:ref('reader')});
action('Organization','deleteOrganization',{id:ref('org')});
action('User','getUser',{id:ref('member')});
action('User','deleteBatchUsers',{ids:[ref('second'),ref('third')]});
action('User','getBatchUsers',{ids:[ref('second'),ref('third')]});
action('Permission','getPermission',{id:ref('write')},{error:true});
action('Role','deleteBatchRoles',{ids:[ref('reader')]});
action('Role','getRole',{id:ref('reader')},{error:true});
action('User','updateUser',{id:'00000000-0000-4000-8000-000000000000',firstName:'Missing'},{error:true});
action('Permission','deletePermission',{id:'00000000-0000-4000-8000-000000000000'});
action('User','createUser',{email:'invalid',password:'unused',firstName:'Invalid',lastName:'Member',roles:[]},{error:true});
action('Role','updateRole',{id:ref('batchRoleOne'),name:null},{error:true});
const expected=[];
for(const item of actions){
 currentAction=item;
 let result;
 try{result=await services[item.service][item.method](resolve(item.input));}
 catch(error){if(!item.error)throw error;expected.push({error:true});continue;}
 assert.ok(!item.error,'expected operation rejection');
 if(item.label)aliases.set(item.label,result.id);
 if(item.labels)item.labels.forEach((label,index)=>aliases.set(label,result[index].id));
 expected.push({ok:result??null});
}
const native=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-iam-base','--example','directory_parity'],{cwd:root,input:JSON.stringify({actions}),encoding:'utf8',timeout:300000}));
assert.equal(native.length,expected.length);
for(let index=0;index<actions.length;index++)assert.deepEqual(native[index],JSON.parse(JSON.stringify(normalize(expected[index]))),`IAM operation ${index}: ${actions[index].service}.${actions[index].method}`);
console.log(`PASS ${actions.length} actual TypeScript/native IAM directory operations: configured mappers, single/batch CRUD, nested role/permission hydration, organization membership, scoped deletion, missing rows and surfaced grants`);

for(const family of ['user','role','permission','organization','discovery','compliance'])actual.load(path.join(root,`blueprint/iam-base/api/routes/${family}.routes.ts`));
const contracts=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-iam-base','--example','directory_parity'],{cwd:root,input:JSON.stringify({contracts:true}),encoding:'utf8',timeout:300000}));
assert.equal(contracts.routes.length,captured.length);
let schemaChecks=0;
function schemaLayout(expected,native,label) {
 const kind=value=>Array.isArray(value?.type)&&value.type.length===1?value.type[0]:value?.type;
 assert.equal(kind(native),kind(expected),label+' type');
 if(expected?.format&&expected.format!=='double')assert.equal(native.format,expected.format,label+' format');
 if(expected?.enum)assert.deepEqual([...native.enum].sort(),[...expected.enum].sort(),label+' enum');
 if(expected?.properties) {
  assert.deepEqual(Object.keys(native.properties).sort(),Object.keys(expected.properties).sort(),label+' fields');
  assert.deepEqual([...(native.required??[])].sort(),[...(expected.required??[])].sort(),label+' required');
  for(const [field,schema]of Object.entries(expected.properties)) {
   // The TS OpenAPI helper describes unknown values as objects; runtime unknown
   // accepts every JSON value. Native contracts retain that runtime acceptance.
   if(field!=='providerFields')schemaLayout(schema,native.properties[field],label+'.'+field);
  }
 }
 if(expected?.items)schemaLayout(expected.items,native.items,label+'[]');
}
for(const expected of captured) {
 const route=contracts.routes.find(route=>route.path===expected.path&&route.method===expected.method);assert.ok(route,`${expected.method} ${expected.path}`);
 assert.equal(route.name,expected.options.name);
 assert.equal(route.access,expected.options.access);
 if(expected.options.auth)assert.equal(route.auth.method,expected.options.auth.hmac?'hmac':'jwt');
 else assert.equal(route.auth,undefined);
 assert.deepEqual(route.request.params.map(field=>field.name).sort(),Object.keys(expected.options.params??{}).sort());
 assert.deepEqual(route.request.query.map(field=>field.name).sort(),Object.keys(expected.options.query??{}).sort());
 assert.equal(route.request.body!=null,expected.options.body!==undefined);
 for(const status of Object.keys(expected.options.responses).map(Number))assert.ok(route.responses.some(response=>response.statusCode===status),`${route.name} status ${status}`);
 if(expected.options.auth?.allowedRoles)assert.deepEqual([...route.rbac.allowedRoles].sort(),[...expected.options.auth?.allowedRoles].sort());
 if(expected.options.auth?.allowedPermissions)assert.deepEqual([...route.rbac.allowedPermissions].sort(),[...expected.options.auth?.allowedPermissions].sort());
 if(expected.options.body) {
  schemaLayout(actual.validator.openapi(expected.options.body),route.request.body.schema.jsonSchema,route.name+' request');schemaChecks++;
 }
 for(const [status,schema]of Object.entries(expected.options.responses)) {
  if(typeof schema==='object'&&schema!==null&&!('safeParse' in schema)) {
   const native=route.responses.find(response=>response.statusCode===Number(status)).body?.schema.jsonSchema;
   schemaLayout(actual.validator.openapi(schema),native,route.name+' response '+status);schemaChecks++;
  }
 }

}
console.log(`PASS ${captured.length} actual TypeScript/native IAM route contracts: paths, methods, names, authentication, role/permission policy, params/query/body fields, required fields and declared response statuses`);

console.log(`PASS ${schemaChecks} actual IAM request/response schema layouts: nested DTOs, required fields, scalar/array types, date/UUID/email formats and organization status enum`);

const metadata=actual.load(path.join(root,'framework/core/src/persistence/complianceTypes.ts'));
const baseProperties=readFileSync(path.join(root,'blueprint/core/persistence/sql.base.properties.ts'),'utf8');
const baseLevels=[...baseProperties.matchAll(/\.compliance\(['"]([^'"]+)['"]\)/g)].map(match=>match[1]);
assert.ok(baseLevels.length>0&&baseLevels.every(level=>level==='none'),'inherited IAM base compliance policy changed');

for(const family of families) {
 const source=readFileSync(path.join(root,`blueprint/iam-base/persistence/entities/${family.toLowerCase()}.entity.ts`),'utf8');
 const levels=[...source.matchAll(/\.compliance\(['"]([^'"]+)['"]\)/g)].map(match=>match[1]);
 assert.ok(levels.length>0);assert.ok(levels.every(level=>level==='none'),family+' configured compliance policy changed');
 metadata.registerEntityCompliance(family,new Map(levels.map((level,index)=>['field'+index,level])));
}
const {ComplianceDataService}=actual.load(path.join(root,'framework/core/src/services/complianceDataService.ts'));
const compliance=new ComplianceDataService({em:{fork:()=>({find:()=>{throw Error('none-classified IAM entities must not be queried for compliance');}})},getMetadata:()=>({getAll:()=>new Map(families.map(className=>[className,{className,properties:{id:{}}}]))})},collector,{User:'id'});
const exported=await compliance.export('fixture-user'),erased=await compliance.erase('fixture-user');
assert.deepEqual(JSON.parse(JSON.stringify(exported)),{userId:'fixture-user',entities:{},failures:[]});
assert.deepEqual(JSON.parse(JSON.stringify(erased)),{entitiesAffected:[],recordsDeleted:0,failures:[]});
console.log('PASS actual configured TypeScript IAM compliance policy: every entity scalar is classified none; export/erase return no data and perform no entity reads');

const classifiedRows={
 User:[{id:'subject',email:'subject@fixture.test',firstName:'Classified',unclassified:'omit',roles:[]},{id:'foreign',email:'foreign@fixture.test',firstName:'Foreign',unclassified:'omit',roles:[]}],
 Role:[{id:'owned-role',owner:'subject',label:'Classified role',permissions:[]},{id:'foreign-role',owner:'foreign',label:'Foreign role',permissions:[]}]
};
const nativeCompliance=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-iam-base','--example','directory_compliance'],{cwd:root,encoding:'utf8',input:JSON.stringify(classifiedRows),timeout:180000}));
metadata.registerEntityCompliance('User',new Map([['id','none'],['email','pii'],['firstName','phi'],['unclassified','none']]));
metadata.registerEntityCompliance('Role',new Map([['id','none'],['owner','none'],['label','pci']]));
const classifiedStore=structuredClone(classifiedRows);
const configuredCompliance=new ComplianceDataService({
 getMetadata:()=>({getAll:()=>new Map(Object.entries(classifiedStore).map(([className,rows])=>[className,{className,properties:Object.fromEntries(Object.keys(rows[0]).map(key=>[key,{}]))}]))}),
 em:{fork:()=>({find:async(entity,filter)=>classifiedStore[entity].filter(row=>Object.entries(filter).every(([key,value])=>row[key]===value)),remove(row){for(const [family,records]of Object.entries(classifiedStore))classifiedStore[family]=records.filter(candidate=>candidate!==row);},flush:async()=>{}})}
},collector,{User:'id',Role:'owner'});
const actualCompliance=[await configuredCompliance.export('subject'),await configuredCompliance.erase('subject'),await configuredCompliance.export('subject'),await configuredCompliance.export('foreign'),await configuredCompliance.erase('subject')];
assert.deepEqual(nativeCompliance,JSON.parse(JSON.stringify(actualCompliance)));
console.log('PASS 5 actual TS/native application-configured compliance operations: PII/PHI/PCI projection, explicit subject links, full-row erasure, unrelated subjects and repeated erasure');

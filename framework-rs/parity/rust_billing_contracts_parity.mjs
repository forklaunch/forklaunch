import assert from 'node:assert/strict';
import {assertJsonSnapshot} from './rust_json_snapshot.mjs';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
import {createRequire} from 'node:module';
import {readFileSync} from 'node:fs';
import {realpath} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
const captured=[];const cache=new Map();
const handlers=new Proxy({}, {get:(_,method)=>(_validator,_path,options,_handler)=>({method,options})});
function router(prefix) {return new Proxy({}, {get:(_,method)=>(suffix,handler)=>{assert.equal(method,handler.method);captured.push({method:method.toUpperCase(),path:prefix+suffix,options:handler.options});return handler;}});}
const schema={PLATFORM_SYSTEM_ROLES:new Set(['system']),ROLES:{ADMIN:"admin"},handlers,forklaunchRouter:router,schemaValidator:{array:value=>[value],number:'Number',record:(key,value)=>({record:[key,value]}),unknown:'Unknown'},unknown:'Unknown',string:'String',number:'Number',boolean:'Boolean',IdSchema:{id:'String'},IdsSchema:{ids:{optional:['String']}},array:value=>[value],optional:value=>({optional:value}),record:(key,value)=>({record:[key,value]}),enum_:value=>({enum:Object.values(value)})};
const tokens=new Proxy({}, {get:(_,key)=>key});const ci={resolve:()=>({}),scopedResolver:()=>()=>({})};
function load(file) {
 if(cache.has(file))return cache.get(file);
 const exports={};cache.set(file,exports);
 const compiled=ts.transpileModule(readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 vm.runInNewContext(compiled,{exports,require(spec){
  if(spec==='@forklaunch/blueprint-core')return schema;
  if(spec==='../../bootstrapper')return {ci,tokens};
  if(spec.includes('/domain/mappers/'))return new Proxy({}, {get:()=>({schema:{}})});
  if(spec.includes('/enum/'))return load(path.resolve(path.dirname(file),spec+'.ts'));
  if(spec.startsWith('../controllers/'))return load(path.resolve(path.dirname(file),spec+'.ts'));
  if(spec.endsWith('/domain/schemas'))return new Proxy({}, {get:()=>new Proxy({}, {get:()=>()=>({})})});
  if(spec==='@forklaunch/validator/zod')return validator;
  if(spec==='stripe')return {};
  if(spec==='@forklaunch/core/http')return {};
  if(spec==='../platformEvents')return {platformEventHandlers:{}};
  throw Error('unexpected runtime import '+spec);
 }});return exports;
}

const validation=actualValidator(root),validator=validation.statics;
const fieldShape=field=>Object.fromEntries(Object.entries(field).filter(([key,value])=>["type","format","enum","items"].includes(key)&&!(key==="format"&&value==="double")).map(([key,value])=>[key,key==="type"&&Array.isArray(value)&&value.length===1?value[0]:key==="items"?fieldShape(value):value]));
const canonicalSchema=value=>Array.isArray(value)?Array.from(value,canonicalSchema):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([key,item])=>[key,key==='type'&&Array.isArray(item)&&item.length===1?item[0]:canonicalSchema(item)])):value;
const families=['plan','subscription','checkoutSession','paymentLink','billingPortal'];
const {toPrettyCamelCase}=load(path.join(root,'framework/common/src/camelCase.ts'));
const simpleNative=route=>({method:route.method,path:route.path,operation:route.operationName,access:route.access,params:route.request.params.map(field=>field.name).sort(),query:route.request.query.map(field=>field.name).sort(),body:route.request.body!=null,statuses:route.responses.map(response=>response.statusCode).sort()});
const simpleTs=route=>({method:route.method,path:route.path.replace(/\/$/,''),operation:toPrettyCamelCase(route.options.name),access:route.options.access,params:Object.keys(route.options.params??{}).sort(),query:Object.keys(route.options.query??{}).sort(),body:route.options.body!==undefined,statuses:Object.keys(route.options.responses).map(Number).sort()});
let count=0,schemaCount=0;
const snapshots=[];
for(const stripe of [false,true]){
 captured.length=0;
 const variant=stripe?'stripe':'base';
 for(const family of [...families,'compliance',...(stripe?['webhook','platformEvents']:[])])load(path.join(root,`blueprint/billing-${variant}/api/routes/${family}.routes.ts`));
 const native=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-interface','--example','module_contract',...(stripe?['--','--stripe']:[])],{cwd:root,encoding:'utf8',timeout:120000}));
 assert.equal(native.routes.length,captured.length);
 // Record policy, not unresolved DI tokens or Set objects that JSON would erase.
 const baseline={variant,routes:captured.map(route=>({...simpleTs(route),name:route.options.name,auth:route.options.access==='public'?undefined:{method:route.options.auth?.hmac?'hmac':'jwt',allowedRoles:[...(route.options.auth?.allowedRoles??[])]}})),schemas:{}};
 snapshots.push(baseline);
 for(const route of captured){const expected=simpleTs(route),actual=native.routes.find(candidate=>candidate.operationName===expected.operation);assert.ok(actual,expected.operation);assert.deepEqual(simpleNative(actual),expected);assert.equal(actual.name,route.options.name);if(route.options.access!=='public')assert.equal(actual.auth.method,route.options.auth?.hmac?'hmac':'jwt');if(route.options.auth?.allowedRoles)assert.deepEqual(actual.rbac.allowedRoles,[...route.options.auth.allowedRoles]);count++;}
 const enums={};
 for(const name of ['planCadence','currency','billingProvider','paymentMethod','party','status']){
  const directory=stripe&&!['party','status'].includes(name)?'blueprint/implementations/billing/stripe':'blueprint/billing-'+variant;
  const values=load(path.join(root,directory,'domain/enum',name+'.enum.ts'));enums[name]=values[Object.keys(values).find(key=>key.endsWith('Enum'))];
 }
 for(const family of families){
  const name=family[0].toUpperCase()+family.slice(1),source=load(path.join(root,`blueprint/implementations/billing/${variant}/domain/schemas/zod/${family}.schema.ts`));
  const args=stripe?(family==='subscription'?[enums.party]:['checkoutSession','paymentLink'].includes(family)?[enums.status]:[]):family==='plan'?[enums.planCadence,enums.currency,enums.billingProvider]:family==='subscription'?[enums.party,enums.billingProvider]:['checkoutSession','paymentLink'].includes(family)?[enums.paymentMethod,enums.currency,enums.status]:[];
  for(const form of ['Create','Update','Read']){
   if(form==='Update'&&family==='checkoutSession')continue;
   const sourceName=(form==='Read'?'':form)+name+'Schema';let expected=source[sourceName];
   if(!stripe&&form!=='Create')expected=expected({uuidId:true});
   if(typeof expected==='function')expected=expected(...args);
   const operation=toPrettyCamelCase((form==='Read'?'Get':form)+' '+(family==='billingPortal'?'Billing Portal Session':name.replace(/[A-Z]/g,letter=>' '+letter).trim()));
   const route=native.routes.find(route=>route.operationName===operation);assert.ok(route,operation);
   const actual=form==='Read'?route.responses[0].body.schema.jsonSchema:route.request.body.schema.jsonSchema;
   expected=validation.validator.openapi(expected);
   baseline.schemas[sourceName]=expected;
   assert.deepEqual(Object.keys(actual.properties).sort(),Object.keys(expected.properties).sort(),variant+' '+sourceName+' properties');
   assert.deepEqual([...(actual.required??[])].sort(),[...(expected.required??[])].sort(),variant+' '+sourceName+' required');
   for(const [key,field]of Object.entries(expected.properties))assert.deepEqual(fieldShape(actual.properties[key]),JSON.parse(JSON.stringify(fieldShape(field))),variant+' '+sourceName+' '+key);
   if(stripe&&family==='paymentLink'&&form==='Create')assert.deepEqual(canonicalSchema(actual.properties.stripeFields),canonicalSchema(expected.properties.stripeFields),'Stripe PaymentLink required nested provider schema');
   schemaCount++;
  }
 }
}
await assertJsonSnapshot(root,'rust_billing_contracts.snap.json',snapshots);
console.log(`PASS ${count} actual TypeScript/Rust billing route contracts and ${schemaCount} actual validator/OpenAPI DTO schemas: methods, paths, names, HMAC, params/query/body, response statuses, required fields and enums`);

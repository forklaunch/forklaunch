import assert from 'node:assert/strict';
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
const schema={handlers,forklaunchRouter:router,schemaValidator:{},unknown:'Unknown',string:'String',number:'Number',boolean:'Boolean',IdSchema:{id:'String'},IdsSchema:{ids:{optional:['String']}},array:value=>[value],optional:value=>({optional:value}),record:(key,value)=>({record:[key,value]}),enum_:value=>({enum:Object.values(value)})};
const tokens=new Proxy({}, {get:(_,key)=>key});const ci={resolve:()=>({}),scopedResolver:()=>()=>({})};
function load(file) {
 if(cache.has(file))return cache.get(file);
 const exports={};cache.set(file,exports);
 const compiled=ts.transpileModule(readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 vm.runInNewContext(compiled,{exports,require(spec){
  if(spec==='../../schema')return schema;
  if(spec==='../../bootstrapper')return {ci,tokens};
  if(spec==='@mikro-orm/core')return {NotFoundError:class extends Error{},UniqueConstraintViolationException:class extends Error{}};
  if(spec.includes('/domain/schemas/checkout.schema'))return {CheckoutResultSchema:{},ShippingAddressSchema:{}};
  if(spec.includes('/domain/schemas/catalogImport.schema'))return load(path.resolve(path.dirname(file),spec+'.ts'));
  if(spec==='stripe')return {default:{webhooks:{}}};
  if(spec.includes('/domain/schemas/webhook.schema'))return load(path.resolve(path.dirname(file),spec+'.ts'));
  if(spec==='node:crypto')return require(spec);
  if(spec==='@forklaunch/interfaces-ecommerce/types')return {OrderStatus:Object.fromEntries(['pending','paid','fulfilled','shipped','delivered','cancelled'].map(value=>[value.toUpperCase(),value]))};
  if(spec==='@forklaunch/implementation-ecommerce-base/services')return {IllegalOrderTransitionError:class extends Error{},CartValidationError:class extends Error{}};
  if(spec.includes('/domain/mappers/'))return new Proxy({}, {get:()=>({schema:{}})});
  if(spec.includes('/domain/enum/'))return {PaymentProvider:{STRIPE:'stripe',PAYPAL:'paypal'}};
  if(spec.startsWith('../controllers/')||spec.includes('/domain/schemas/productSearch.schema'))return load(path.resolve(path.dirname(file),spec+'.ts'));
  if(spec.includes('/domain/schemas/catalog.schema'))return {CatalogPageSchema:{}};
  throw Error('unexpected runtime import '+spec);
 }});return exports;
}
for(const name of ['product','variant','inventory','cart','order','payment','catalogImport','checkout','webhook'])load(path.join(root,`blueprint/ecommerce-stripe/api/routes/${name}.routes.ts`));
const native=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-interface','--example','module_contract'],{cwd:root,encoding:'utf8',timeout:120000}));
assert.equal(native.routes.length,captured.length);
const {toPrettyCamelCase}=load(path.join(root,"framework/common/src/camelCase.ts"));
const simpleNative=route=>({method:route.method,path:route.path,operation:route.operationName,access:route.access,headers:route.request.headers.map(field=>field.name).sort(),params:route.request.params.map(field=>field.name).sort(),query:route.request.query.map(field=>field.name).sort(),body:route.request.body!==null&&route.request.body!==undefined,statuses:route.responses.map(response=>response.statusCode).sort()});
// Express accepts a trailing slash by default; Rust contracts canonicalize it.
const simpleTs=route=>({method:route.method,path:route.path.replace(/\/$/,''),operation:toPrettyCamelCase(route.options.name),access:route.options.access,headers:Object.keys(route.options.requestHeaders??{}).sort(),params:Object.keys(route.options.params??{}).sort(),query:Object.keys(route.options.query??{}).sort(),body:route.options.body!==undefined,statuses:Object.keys(route.options.responses).map(Number).sort()});
for(const route of captured) {
 const expected=simpleTs(route);const actual=native.routes.find(candidate=>candidate.operationName===expected.operation);assert(actual,expected.operation);
 assert.deepEqual(simpleNative(actual),expected,expected.operation);
 assert.equal(actual.name,route.options.name);
 if(route.options.auth?.hmac)assert.equal(actual.auth.method,'hmac');else assert.equal(actual.auth??null,null);
 for(const response of actual.responses)if(response.body?.kind==='json')assert(response.body.schema.jsonSchema.type);
}
const payment=native.routes.find(route=>route.operationName==='createPayment');
assert.deepEqual(Object.keys(payment.request.body.schema.jsonSchema.properties).sort(),['amountCents','currency','orderId','provider']);
assert(!('providerRef' in payment.request.body.schema.jsonSchema.properties));
const created=payment.responses.find(response=>response.statusCode===200).body.schema.jsonSchema;
assert(created.properties.clientSecret);assert(!created.required.includes('clientSecret'));
const get=native.routes.find(route=>route.operationName==='getPayment');assert(!get.responses[0].body.schema.jsonSchema.properties.clientSecret);
const catalog=native.routes.find(route=>route.operationName==='listCatalog');assert.equal(catalog.request.query.find(field=>field.name==='ids').schema.jsonSchema.type,'array');
console.log(`PASS ${captured.length} actual TypeScript/Rust core ecommerce route contracts: methods, paths, names, HMAC, params/query/body and response statuses`);

const imported=native.routes.find(route=>route.operationName==='importCatalog').request.body.schema.jsonSchema;
assert.deepEqual(imported.required,['products']);
assert(!('id' in imported.properties.products.items.properties));
assert(!('productId' in imported.properties.products.items.properties.variants.items.properties));
assert(!('id' in imported.properties.products.items.properties.variants.items.properties));

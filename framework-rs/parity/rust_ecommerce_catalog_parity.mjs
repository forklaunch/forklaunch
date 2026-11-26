import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
const clone=value=>JSON.parse(JSON.stringify(value));
const products=[['z','Coffee Beans','2026-01-01'],['a','Tea Leaves','2026-01-01'],['b','Coffee Cups','2026-01-02'],['c','Unstocked','2026-01-03']].map(([id,title,date])=>({id,externalId:`external-${id}`,handle:`handle-${id}`,title,createdAt:`${date}T00:00:00.000Z`}));
const variants=[['v1','z',100,{Size:'Small'}],['v2','z',200,{Size:'Large'}],['v3','a',150,{Size:'Small'}],['v4','b',300,{Size:'Large'}],['v5','c',50,undefined]].map(([id,productId,priceCents,optionValues])=>({id,productId,externalId:`external-${id}`,title:id,priceCents,requiresShipping:true,...(optionValues?{optionValues}:{})}));
const inventory=[['v1',5],['v2',0],['v3',2],['v4',0]].map(([variantId,stock],i)=>({id:`i${i}`,variantId,stock}));
const tables={Product:products,Variant:variants,Inventory:inventory};
function matches(row,where={}) {return Object.entries(where).every(([key,value])=>value&&typeof value==='object'?('$in' in value?value.$in.includes(row[key]):'$ilike' in value?new RegExp('^'+value.$ilike.replace(/[.*+?^${}()|[\]\\]/g,'\\$&').replaceAll('%','.*').replaceAll('_','.')+'$','isu').test(row[key]):false):row[key]===value);}
const calls=[];
class EntityManager {
 async findAll(entity,{where}={}) {calls.push({operation:'findAll',entity,where});return clone(tables[entity].filter(row=>matches(row,where)).sort((a,b)=>a.id.localeCompare(b.id)));}
 async find(entity,where,options={}) {calls.push({operation:'find',entity,where,options});return clone(tables[entity].filter(row=>matches(row,where)).sort((a,b)=>a.id.localeCompare(b.id)));}
 async findOneOrFail(entity,where) {const row=(await this.find(entity,where))[0];if(!row)throw Error('missing');return row;}
 async findAndCount(entity,where,options) {calls.push({operation:'findAndCount',entity,where,options});const rows=tables[entity].filter(row=>matches(row,where)).sort((a,b)=>a.createdAt.localeCompare(b.createdAt)||a.id.localeCompare(b.id));return [clone(rows.slice(options.offset,options.offset+options.limit)),rows.length];}
}
async function load(relative,imports) {
 const source=await readFile(path.join(root,relative),'utf8');
 const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 const exports={};vm.runInNewContext(compiled,{exports,require(spec){if(spec in imports)return imports[spec];throw Error('unexpected runtime import '+spec);}});return exports;
}
const em=new EntityManager();const services={};const entities=Object.fromEntries(Object.keys(tables).map(name=>[name,name]));
for(const name of Object.keys(tables)) {
 const classes=await load(`blueprint/implementations/ecommerce/base/services/${name.toLowerCase()}.service.ts`,{'@mikro-orm/core':{EntityManager},'../persistence/entities':entities,'@forklaunch/core/http':{evaluateTelemetryOptions:()=>({enabled:{}})}});
 services[name+'Service']=new classes['Base'+name+'Service'](em,{}, {},{[name+'Mapper']:{entity:name,toDto:clone}});
}
const {CatalogLookupService}=await load('blueprint/ecommerce-stripe/domain/services/catalogLookup.service.ts',{'../../persistence/entities/product.entity':{Product:'Product'},'../../persistence/entities/variant.entity':{Variant:'Variant'},'../../persistence/entities/inventory.entity':{Inventory:'Inventory'}});
services.CatalogLookupService=new CatalogLookupService(em);
const handlers=new Proxy({}, {get:()=> (_validator,_path,_options,handler)=>handler});
const schema={handlers,string:{},number:{},IdSchema:{},schemaValidator:{},array:value=>value,optional:value=>value};
const tokens=new Proxy({}, {get:(_,key)=>key});
const ci={resolve:token=>token==='HMAC_SECRET_KEY'?'fixture-secret':{debug(){}},scopedResolver:token=>()=>services[token]};
const controller=await load('blueprint/ecommerce-stripe/api/controllers/product.controller.ts',{'../../schema':schema,'../../bootstrapper':{ci,tokens},'../../domain/schemas/productSearch.schema':{ProductSearchQuerySchema:{}},'../../domain/schemas/catalog.schema':{CatalogPageSchema:{}},'../../domain/mappers/product.mappers':{ProductMapper:{schema:{}},CreateProductMapper:{schema:{}},UpdateProductMapper:{schema:{}}}});
const searches=[{}, {ids:[]},{ids:['z']},{ids:['missing']},{title:'COFFEE'},{title:'C_ps'},{title:'%LEAVES'},{minPriceCents:150},{maxPriceCents:100},{minPriceCents:500},{minPriceCents:200,maxPriceCents:100},{inStock:true},{inStock:false},{inStock:false,ids:['z','a']},{inStock:true,ids:[]},{inStock:true,title:'Coffee'},{optionName:'Size',optionValue:'Large'},{optionName:'Size',optionValue:'missing'},{optionName:'Size'},{optionValue:'Small'},{optionName:'Size',optionValue:'Small',maxPriceCents:100,inStock:true}];
const catalogs=[{}, {limit:1},{limit:2,offset:1},{limit:2,offset:3},{offset:1000},{limit:0,offset:-4},{limit:-10},{limit:200},{ids:[]},{ids:['z','b']},{ids:['missing']},{title:'COFFEE',limit:1,offset:1},{title:'C_ps'},{title:'%LEAVES'}];
const cases=[...searches.map(query=>({operation:'search',query})),...catalogs.map(query=>({operation:'catalog',query}))];
const expected=[];
for(const testcase of cases) {
 let value;const before=calls.length;
 await controller[testcase.operation==='search'?'listProducts':'listCatalog']({query:clone(testcase.query)},{status(code){assert.equal(code,200);return this;},json(result){value=clone(result);}});
 expected.push({ok:value});
 if(testcase.operation==='catalog') {
   const pageCall=calls.slice(before).find(call=>call.operation==='findAndCount');assert(pageCall);assert.equal(pageCall.options.limit,value.limit);assert.equal(pageCall.options.offset,value.offset);
   for(const call of calls.slice(before).filter(call=>call.entity==='Variant'||call.entity==='Inventory')) assert(Object.values(call.where??{}).some(value=>value&&typeof value==='object'&&'$in' in value),'catalog must scope variant/stock reads');
 }
}
const records=Object.entries(tables).flatMap(([kind,rows])=>rows.map(record=>({kind:kind.toLowerCase(),record})));
const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-base','--example','catalog_parity'],{cwd:root,input:JSON.stringify({records,cases}),encoding:'utf8',timeout:120000}));
assert.equal(actual.length,expected.length);for(let i=0;i<cases.length;i++)assert.deepEqual(actual[i],expected[i],JSON.stringify(cases[i]));
console.log(`PASS ${cases.length} actual TypeScript/Rust catalog controller cases: price/options/stock, empty ids, SQL page bounds, stable ordering and optional inventory`);

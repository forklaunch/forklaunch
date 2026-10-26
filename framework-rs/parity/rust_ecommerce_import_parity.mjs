import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
class NotFoundError extends Error{}
const tables={product:[],variant:[],inventory:[]};let serial=0,handler;
const clean=value=>Object.fromEntries(Object.entries(value).filter(([,value])=>value!==undefined));
function get(kind,field,value){const row=tables[kind].find(row=>row[field]===value);if(!row)throw new NotFoundError();return structuredClone(row);}
function create(kind,input){const row={id:`id-${++serial}`,...clean(input)};tables[kind].push(row);return structuredClone(row);}
function update(kind,input){const row=tables[kind].find(row=>row.id===input.id);if(!row)throw new NotFoundError();Object.assign(row,clean(input));return structuredClone(row);}
const services={ProductService:{getProductByExternalId:({externalId})=>get('product','externalId',externalId),createProduct:input=>create('product',input),updateProduct:input=>update('product',input)},VariantService:{getVariantByExternalId:({externalId})=>get('variant','externalId',externalId),createVariant:input=>create('variant',{requiresShipping:true,...clean(input)}),updateVariant:input=>update('variant',input)},InventoryService:{getInventory:({variantId})=>get('inventory','variantId',variantId),createInventory:input=>create('inventory',input)}};
const tokens=new Proxy({}, {get:(_,key)=>key});const ci={resolve:()=>({info(){}}),scopedResolver:token=>()=>services[token]};
const source=await readFile(path.join(root,'blueprint/ecommerce-stripe/api/controllers/catalogImport.controller.ts'),'utf8');
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:{},require(spec){
 if(spec==='@mikro-orm/core')return{NotFoundError};
 if(spec==='../../bootstrapper')return{ci,tokens};
 if(spec==='../../schema')return{handlers:{post(_validator,_path,_options,callback){handler=callback;}},array:value=>value,number:{},schemaValidator:{}};
 if(spec.includes('catalogImport.schema'))return{ImportProductSchema:{}};throw Error('unexpected runtime import '+spec);
}});
const base={externalId:'p1',handle:'beans',title:'Beans',sourceUrl:'https://source.example/beans',tags:['food'],options:[{name:'Size',isPackQuantity:false,values:['Large']}],images:[{src:'https://source.example/image',position:1}],variants:[{externalId:'v1',title:'Large',sku:'sku-1',priceCents:100,initialStock:8,optionValues:{Size:'Large'}},{externalId:'v2',title:'Small',priceCents:50}]};
const actions=[{body:{products:[]}},{body:{products:[base]}},{operation:'adjust',externalId:'v1',delta:-3},{body:{products:[base]}},{body:{products:[{...base,title:'New Beans',variants:[{...base.variants[0],priceCents:150,initialStock:999}]}]}},{body:{products:[{externalId:'p2',handle:'tea',title:'Tea',variants:[{externalId:'v3',title:'Tea box',priceCents:200,requiresShipping:false,initialStock:2}]}]}},{body:{products:[{externalId:'p2',handle:'tea',title:'Tea',variants:[{externalId:'v2',title:'Moved',priceCents:75}]}]}},{body:{products:[{externalId:'p1',handle:'beans',title:'Minimal',variants:[]}]}},{body:{products:[]}}];
function records(){
 const ids=new Map([...tables.product,...tables.variant].map(row=>[row.id,row.externalId]));
 return Object.entries(tables).flatMap(([kind,rows])=>rows.map(row=>{const record=structuredClone(row);if(kind==='inventory')delete record.id;for(const field of ['id','productId','variantId'])if(field in record)record[field]=ids.get(record[field]);return{kind,record};}));
}
const expected=[];
for(const action of actions){let result=null;
 if(action.operation==='adjust'){const variant=get('variant','externalId',action.externalId);const row=tables.inventory.find(row=>row.variantId===variant.id);row.stock+=action.delta;}
 else await handler({body:structuredClone(action.body)},{status(code){assert.equal(code,200);return this;},json(value){result=JSON.parse(JSON.stringify(value));}});
 expected.push({result,records:records()});
}
const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-base','--example','import_parity'],{cwd:root,input:JSON.stringify(actions),encoding:'utf8',timeout:120000}));
const sort=result=>({...result,records:result.records.sort((a,b)=>`${a.kind}:${a.record.externalId??a.record.variantId}`.localeCompare(`${b.kind}:${b.record.externalId??b.record.variantId}`))});
assert.deepEqual(actual.map(sort),expected.map(sort));
console.log(`PASS ${actions.length} actual TypeScript/Rust catalog import operations: external-ID upserts, omitted fields, variant moves, default shipping/stock and preservation of live stock`);

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
const enums={OrderStatus:Object.fromEntries(['pending','paid','fulfilled','shipped','delivered','cancelled'].map(v=>[v.toUpperCase(),v])),PaymentStatus:{PENDING:'pending',SUCCEEDED:'succeeded',FAILED:'failed'}};
const clone=v=>v===undefined?undefined:structuredClone(v);
// Only the ORM/mapper boundary is substituted; all service business rules below
// execute the repository's actual TypeScript classes. PostgreSQL races/RLS are
// covered separately by the Rust integration test against a real database.
function matches(row,where={}) {return Object.entries(where).every(([k,v])=>v&&typeof v==='object'?('$in' in v?v.$in.includes(row[k]):'$gte' in v?row[k]>=v.$gte:'$ilike' in v?new RegExp('^'+v.$ilike.replace(/[.*+?^${}()|[\]\\]/g,'\\$&').replaceAll('%','.*').replaceAll('_','.')+'$','isu').test(row[k]):false):row[k]===v);}
class EntityManager {
 constructor(){this.rows=new Map();}
 table(entity){if(!this.rows.has(entity))this.rows.set(entity,new Map());return this.rows.get(entity);}
 async transactional(fn){return fn(this);}
 async persist(row){this.table(row.entity).set(row.id,row);}
 async upsert(row){await this.persist(row);return row;}
 async findAll(entity,{where}={}){return [...this.table(entity).values()].filter(r=>matches(r,where)).sort((a,b)=>a.id.localeCompare(b.id));}
 async findOne(entity,where){return (await this.findAll(entity,{where}))[0]??null;}
 async findOneOrFail(entity,where){const r=await this.findOne(entity,where);if(!r)throw Error('not found');return r;}
 assign(row,update){Object.assign(row,update);}
 async nativeDelete(entity,where){for(const r of await this.findAll(entity,{where}))this.table(entity).delete(r.id);}
 async nativeUpdate(entity,where,update){const rows=await this.findAll(entity,{where});for(const row of rows)for(const [k,v]of Object.entries(update))row[k]=v&&v.raw?row[k]+v.delta:v;return rows.length;}
}
const entityNames=['Product','Variant','Inventory','Cart','Order','Payment'];
const entities=Object.fromEntries(entityNames.map(n=>[n,n]));const em=new EntityManager();const services={};
for(const name of entityNames){
 const source=await readFile(path.join(root,'blueprint/implementations/ecommerce/base/services',name.toLowerCase()+'.service.ts'),'utf8');
 const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 const exports={};vm.runInNewContext(compiled,{exports,require(spec){if(spec==='@mikro-orm/core')return {EntityManager,raw:(_,args)=>({raw:true,delta:args[0]})};if(spec==='@forklaunch/interfaces-ecommerce/types')return enums;if(spec.endsWith('/persistence/entities'))return entities;if(spec==='@forklaunch/core/http')return {evaluateTelemetryOptions:()=>({enabled:{}})};throw Error('unexpected runtime import '+spec);},setTimeout,clearTimeout});
 const mapper={entity:name,toDto(row){const {entity,...dto}=row;return clone(dto);}};
 const mappers={[name+'Mapper']:mapper,['Create'+name+'Mapper']:{async toEntity(input){return {entity:name,...(name==='Cart'?{status:'open',items:[]}:name==='Order'||name==='Payment'?{status:'pending'}:name==='Variant'?{requiresShipping:true}:{}),...clone(input)};}},['Update'+name+'Mapper']:{async toEntity(input){const row=await em.findOneOrFail(name,{id:input.id});return {...row,...clone(input)};}}};
 services[name]=new exports['Base'+name+'Service'](em,{}, {},mappers);
}
const actions=[];const add=(service,method,input={})=>actions.push({service,method,input});
add('Product','createProduct',{id:'p1',externalId:'external',handle:'coffee',title:'Coffee Beans',tags:['organic']});
for(const input of [{},{ids:[]},{ids:['missing']},{title:'coffee'},{title:'B_ans'},{title:'%BEANS'}])add('Product','listProducts',input);
add('Product','getProduct',{id:'p1'});add('Product','getProductByHandle',{handle:'coffee'});add('Product','getProductByExternalId',{externalId:'external'});
add('Product','updateProduct',{id:'p1',title:'Tea'});add('Product','getProduct',{id:'missing'});
add('Variant','createVariant',{id:'v1',productId:'p1',externalId:'external-v',title:'Large',priceCents:1999,optionValues:{Size:'Large'}});
add('Variant','getVariant',{id:'v1'});add('Variant','getVariantByExternalId',{externalId:'external-v'});add('Variant','listVariants',{ids:[]});add('Variant','listVariantsByProduct',{productId:'p1'});add('Variant','updateVariant',{id:'v1',priceCents:2000,requiresShipping:false});
add('Inventory','createInventory',{id:'i1',variantId:'v1',stock:3});
for(const delta of [-2,-2,4,0,-5])add('Inventory','adjustStock',{variantId:'v1',delta});
add('Inventory','getInventory',{variantId:'v1'});add('Inventory','adjustStock',{variantId:'missing',delta:1});
for(const requested of [0,1,100])add('Inventory','checkStock',{variantId:'v1',requested});
add('Cart','createCart',{id:'c1'});add('Cart','getCart',{id:'c1'});
for(const quantity of [2,3,0,-1,1.5])add('Cart','addItem',{cartId:'c1',variantId:'v1',quantity});
add('Cart','addItem',{cartId:'c1',variantId:'out-of-stock',quantity:1});add('Cart','getCart',{id:'c1'});add('Cart','removeItem',{cartId:'c1',variantId:'v1'});add('Cart','clearCart',{id:'c1'});add('Cart','getCart',{id:'c1'});
const order={id:'o1',items:[{variantId:'v1',quantity:2,unitPriceCents:1999}],shippingAddress:{name:'Person',line1:'Street',city:'Oakland',state:'CA',postalCode:'94601',country:'US'},subtotalCents:3998,discountCents:0,taxCents:320,taxBreakdown:[{jurisdiction:'CA',taxCents:320}],shippingCents:500,giftCardCents:0,totalCents:4818};
add('Order','createOrder',order);add('Order','getOrder',{id:'o1'});add('Order','listOrders',{ids:[]});
for(const to of ['shipped','paid','paid','fulfilled','shipped','cancelled','delivered','cancelled'])add('Order','transitionOrder',{id:'o1',to});
const states=['pending','paid','fulfilled','shipped','delivered','cancelled'];
for(const from of states)for(const to of states){const id=`transition-${from}-${to}`;add('Order','createOrder',{...order,id});for(const step of from==='cancelled'?['cancelled']:states.slice(1,states.indexOf(from)+1))add('Order','transitionOrder',{id,to:step});add('Order','transitionOrder',{id,to});}
add('Payment','createPayment',{id:'pay1',orderId:'o1',amountCents:4818,currency:'usd',providerRef:'ref'});add('Payment','getPayment',{id:'pay1'});
for(const method of ['claimForConfirmation','claimForConfirmation','releaseConfirmationClaim','failPayment','claimForConfirmation','confirmPayment','confirmPayment','failPayment'])add('Payment',method,{providerRef:'ref'});
add('Payment','claimForConfirmation',{providerRef:'missing'});add('Payment','releaseConfirmationClaim',{providerRef:'missing'});add('Payment','confirmPayment',{providerRef:'pay1'});
add('Product','deleteProduct',{id:'p1'});add('Product','deleteProduct',{id:'p1'});add('Variant','getVariant',{id:'v1'});add('Variant','deleteVariant',{id:'v1'});add('Variant','listVariants');
const expected=[];for(const {service,method,input}of actions){try {expected.push({ok:clone(await services[service][method](['claimForConfirmation','releaseConfirmationClaim'].includes(method)?input.providerRef:input))??null});}catch{expected.push({error:true});}}
const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-base','--example','ecommerce_parity'],{cwd:root,input:JSON.stringify(actions),encoding:'utf8',timeout:120000}));
assert.equal(actual.length,expected.length);for(let i=0;i<expected.length;i++)assert.deepEqual(actual[i],expected[i],JSON.stringify(actions[i]));
console.log(`PASS ${actions.length} actual TypeScript/Rust ecommerce operations across all six base services and all 36 order transitions`);

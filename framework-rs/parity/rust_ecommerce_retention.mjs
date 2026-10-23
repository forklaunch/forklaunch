// Actual encrypted MikroORM Order metadata and explicit generic retention policies.
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { betterAuthOrm } from './fixtures/rust_better_auth_orm.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const source = await betterAuthOrm(root, process.env.FORKLAUNCH_TEST_ORM_DEPENDENCIES,
  (specifier, _file, load) => specifier === '@forklaunch/interfaces-ecommerce/types'
    ? load(path.join(root, 'blueprint/interfaces/ecommerce/types/order.service.types.ts')) : undefined, true, 'blueprint/ecommerce-stripe/persistence/entities/order.entity.ts');
const { persistence, core } = source;
const { Order } = source.load('blueprint/ecommerce-stripe/persistence/entities/order.entity.ts');
const { RetentionService } = source.load('framework/core/src/services/retentionService.ts');
persistence.registerEncryptor(new persistence.FieldEncryptor('ecommerce-orm-compliance-fixture-secret'));
const schema = `fl_ecom_retention_${crypto.randomBytes(8).toString('hex')}`;
const orm = await source.MikroORM.init({ entities:[Order], clientUrl:process.env.FORKLAUNCH_TEST_POSTGRES_URL, schema });
const tenant = 'ecommerce-retention-fixture';
const context = callback => core.RequestContext.create(orm.em, () => persistence.withEncryptionContext(tenant, callback));
const now = '2028-01-01T00:00:00.000Z', cutoff = '2021-01-01T00:00:00.000Z';
const records = ['2018-01-01T00:00:00.000Z','2020-01-01T00:00:00.000Z',cutoff].map(createdAt=>({kind:'order',record:{
  id:crypto.randomUUID(),customerId:crypto.randomUUID(),status:'pending',items:[],
  shippingAddress:{name:'Retention Person',line1:'Private Street',city:'Oakland',state:'CA',postalCode:'94601',country:'US'},
  subtotalCents:0,discountCents:0,taxCents:0,taxBreakdown:[],shippingCents:0,giftCardCents:0,totalCents:0,
  createdAt,updatedAt:createdAt
}}));
const cases = [{enabled:false},{dryRun:true},{entities:['Cart']},{},{},{action:'delete',dryRun:true},{action:'delete'},{action:'delete'}];
function normalized(value) {
  const result = JSON.parse(JSON.stringify(value));
  delete result.result.durationMs;
  result.rows.sort((a,b)=>a.id.localeCompare(b.id));
  return result;
}
const RealDate = Date;
try {
  await orm.schema.create();
  await context(async()=>{
    const em = orm.em.fork();
    for(const {record} of records) em.persist(em.create(Order,{...record,createdAt:new Date(record.createdAt),updatedAt:new Date(record.updatedAt)}));
    await em.flush();
  });
  process.env.TZ = 'UTC';
  globalThis.Date = class extends RealDate {
    constructor(...args) { super(...(args.length ? args : [now])); }
    static now() { return new RealDate(now).getTime(); }
  };
  const expected = [];
  const service = new RetentionService(orm,{info(){},warn(){},error(){}});
  for(const testcase of cases) {
    if(testcase.enabled !== false) persistence.registerEntityRetention('Order',{duration:'P7Y',action:testcase.action??'anonymize'});
    const result = await context(()=>service.enforce({tenantIds:[tenant],batchSize:1,dryRun:testcase.dryRun,entities:testcase.entities}));
    const rows = await context(async()=> (await orm.em.fork().find(Order,{})).map(row=>({id:row.id,address:row.shippingAddress.deanon})));
    expected.push(normalized({result,rows}));
  }
  assert.equal(expected[0].result.processed,0);
  assert.equal(expected[1].result.anonymized,2);
  assert.equal(expected[3].result.anonymized,2);
  assert.equal(expected[4].result.processed,0);
  assert.equal(expected[5].result.deleted,2);
  assert.equal(expected[6].rows.length,1);
  globalThis.Date = RealDate;
  const actual = JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-base','--example','retention_parity'],{
    cwd:root,input:JSON.stringify({records,cases,now,cutoff}),encoding:'utf8',timeout:120000,env:{...process.env,CARGO_INCREMENTAL:'0'}
  }));
  for(let index=0;index<cases.length;index++) assert.deepEqual(normalized(actual[index]),expected[index],JSON.stringify(cases[index]));
  console.log(`PASS ${cases.length} actual encrypted ORM/native ecommerce retention comparisons: opt-in policy, exact cutoff, dry run, required address preservation, replay and deletion`);
} finally {
  globalThis.Date = RealDate;
  try { await orm.schema.drop({dropMigrationsTable:true,dropDb:false}); }
  finally { await orm.close(true); }
}

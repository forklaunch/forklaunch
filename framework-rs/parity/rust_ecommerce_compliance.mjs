// Actual encrypted MikroORM Order metadata and generic compliance walker.
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
const { ComplianceDataService } = source.load('framework/core/src/services/complianceDataService.ts');
persistence.registerEncryptor(new persistence.FieldEncryptor('ecommerce-orm-compliance-fixture-secret'));
const schema = `fl_ecom_compliance_${crypto.randomBytes(8).toString('hex')}`;
const orm = await source.MikroORM.init({ entities:[Order], clientUrl:process.env.FORKLAUNCH_TEST_POSTGRES_URL, schema });
const scope = { tenantIds:['ecommerce-compliance-fixture'] };
const context = callback => core.RequestContext.create(orm.em, () => persistence.withEncryptionContext(scope.tenantIds[0], callback));
const customer = crypto.randomUUID(), foreign = crypto.randomUUID();
const records = [customer,customer,foreign].map(customerId => ({kind:'order',record:{
  id:crypto.randomUUID(), customerId, status:'pending', items:[],
  shippingAddress:{name:'Private Person',line1:'Secret Street',city:'Oakland',state:'CA',postalCode:'94601',country:'US'},
  subtotalCents:0,discountCents:0,taxCents:0,taxBreakdown:[],shippingCents:0,giftCardCents:0,totalCents:0,
  createdAt:'2020-01-01T00:00:00.000Z',updatedAt:'2020-01-01T00:00:00.000Z'
}}));
const cases = [
  {operation:'export',subject:customer},
  {operation:'export',subject:records[0].record.id},
  {operation:'export',subject:customer,customer:true},
  {operation:'export',subject:foreign,customer:true},
  {operation:'export',subject:crypto.randomUUID(),customer:true},
  {operation:'erase',subject:customer},
  {operation:'erase',subject:customer,customer:true},
  {operation:'export',subject:customer,customer:true},
  {operation:'erase',subject:customer,customer:true},
  {operation:'export',subject:foreign,customer:true},
  {operation:'erase',subject:records[2].record.id},
  {operation:'export',subject:foreign,customer:true},
];
const clean = value => JSON.parse(JSON.stringify(value));
function normalized(result) {
  const value = clean(result);
  value.entities?.Order?.sort((a,b)=>a.id.localeCompare(b.id));
  return value;
}
try {
  await orm.schema.create();
  await context(async () => {
    const em = orm.em.fork();
    for (const {record} of records) em.persist(em.create(Order,{...record,createdAt:new Date(record.createdAt),updatedAt:new Date(record.updatedAt)}));
    await em.flush();
  });
  const expected = [];
  for (const testcase of cases) {
    const service = new ComplianceDataService(orm,{info(){},warn(){},error(){}},testcase.customer ? {Order:'customerId'} : undefined);
    expected.push(normalized(await context(()=>service[testcase.operation](testcase.subject,scope))));
  }
  const actual = JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-base','--example','compliance_parity'],{
    cwd:root,input:JSON.stringify({records,cases}),encoding:'utf8',timeout:120000,env:{...process.env,CARGO_INCREMENTAL:'0'}
  }));
  for (let index=0;index<cases.length;index++) assert.deepEqual(normalized(actual[index]),expected[index],JSON.stringify(cases[index]));
  console.log(`PASS ${cases.length} actual encrypted ORM/native ecommerce compliance comparisons: default ID mapping, explicit customer scope, classified export, erasure, replay and foreign ownership`);
} finally {
  try { await orm.schema.drop({dropMigrationsTable:true,dropDb:false}); }
  finally { await orm.close(true); }
}

// Actual schemas, configured mappers and both service layers; ORM/cache/provider seams.
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { actualValidator } from './rust_actual_validator_fixture.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
let cases = 0;
for (const flavor of ['zod', 'typebox']) {
  let statics, schemas;
  class EntityManager {}
  const actual = actualValidator(root, (spec, file, load) => {
    if (spec === `@forklaunch/validator/${flavor}`) return statics;
    if (spec === '@forklaunch/blueprint-core') return { schemaValidator: statics.SchemaValidator() };
    if (spec === '@forklaunch/core/mappers') return load(path.join(root, 'framework/core/src/mappers/mapper.ts'));
    if (spec === '../schemas' && file.endsWith('/billingPortal.mappers.ts')) return { BillingPortalSchemas: schemas };
    if (spec.includes('/persistence/entities/billingPortal.entity')) return { BillingPortal: { name: 'BillingPortal' } };
    if (spec === '@mikro-orm/core') return { EntityManager, wrap: entity => ({ toPOJO: () => ({ ...entity }) }) };
    if (spec === '@forklaunch/core/cache') return { createCacheKey: prefix => id => prefix + ':' + id };
    if (spec === '@forklaunch/core/http') return { evaluateTelemetryOptions: () => ({ enabled: {} }) };
    if (spec === '@forklaunch/implementation-billing-base/services') return load(path.join(root, 'blueprint/implementations/billing/base/services/billingPortal.service.ts'));
  });
  statics = actual.load(path.join(root, `framework/validator/src/${flavor}/staticSchemaValidator.ts`));
  schemas = actual.load(path.join(root, `blueprint/implementations/billing/stripe/domain/schemas/${flavor}/billingPortal.schema.ts`)).StripeBillingPortalServiceSchemas;
  const validator = statics.SchemaValidator();
  const mappers = actual.load(path.join(root, 'blueprint/billing-stripe/domain/mappers/billingPortal.mappers.ts'));
  const { StripeBillingPortalService } = actual.load(path.join(root, 'blueprint/implementations/billing/stripe/services/billingPortal.service.ts'));
  for (const backup of [false, true]) {
    let persisted = 0;
    const payloads = [], cached = [];
    const provider = { id: 'bps_owned', object: 'billing_portal.session', url: 'https://owned.test/portal', customer: 'cus_owned' };
    const em = Object.assign(new EntityManager(), {
      create(_entity, fields) { return { id: randomUUID(), ...fields }; },
      persist() { persisted++; return { async flush() {} }; }
    });
    const service = new StripeBillingPortalService(
      { billingPortal: { sessions: { async create(payload) { payloads.push(structuredClone(payload)); return provider; } } } },
      em, { getTtlMilliseconds: () => 60000, async putRecord(record) { cached.push(record.value); } },
      {}, validator, mappers, { enableDatabaseBackup: backup }
    );
    for (const fields of [undefined, {}, { return_url: 'https://owned.test/return' }]) {
      const dto = { customerId: 'cus_owned', expiresAt: new Date('2030-01-01T00:00:00Z'), ...(fields === undefined ? {} : { stripeFields: fields }) };
      assert.equal(validator.parse(validator.schemify(schemas.CreateBillingPortalSchema), dto).ok, true);
      const made = await service.createBillingPortalSession(dto);
      assert.equal(made.customerId, 'cus_owned');
      assert.equal(made.uri, provider.url);
      assert.deepEqual(made.stripeFields, provider);
      assert.equal(cached.at(-1).id, made.id);
      cases++;
    }
    assert.deepEqual(payloads[0], { customer: 'cus_owned' });
    assert.deepEqual(payloads[1], payloads[0]);
    assert.deepEqual(payloads[2], { customer: 'cus_owned', return_url: 'https://owned.test/return' });
    assert.equal(persisted, backup ? 3 : 0);
  }
}
console.log(`PASS ${cases} actual Zod/TypeBox portal create-option cases across configured mappers, cache and backup dispatch`);

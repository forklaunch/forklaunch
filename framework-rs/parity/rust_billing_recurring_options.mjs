// Actual validators, configured mappers and base/Stripe services; owned ORM/provider seams.
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { actualValidator } from './rust_actual_validator_fixture.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
let cases = 0;
for (const flavor of ['zod', 'typebox']) for (const kind of ['plan', 'subscription']) {
  const title = kind === 'plan' ? 'Plan' : 'Subscription';
  let statics, schemas;
  class EntityManager {}
  const actual = actualValidator(root, (spec, file, load) => {
    if (spec === `@forklaunch/validator/${flavor}`) return statics;
    if (spec === '@forklaunch/blueprint-core') return { schemaValidator: statics.SchemaValidator() };
    if (spec === '@forklaunch/core/mappers') return load(path.join(root, 'framework/core/src/mappers/mapper.ts'));
    if (spec === '../schemas' && file.endsWith(`/${kind}.mappers.ts`)) return { [`${title}Schemas`]: schemas };
    if (spec.includes(`/persistence/entities/${kind}.entity`)) return { [title]: { name: title } };
    if (spec === '@mikro-orm/core') return { EntityManager };
    if (spec === '@forklaunch/core/http') return { evaluateTelemetryOptions: () => ({ enabled: {} }) };
    if (spec === '@forklaunch/implementation-billing-base/services') return load(path.join(root, `blueprint/implementations/billing/base/services/${kind}.service.ts`));
  });
  statics = actual.load(path.join(root, `framework/validator/src/${flavor}/staticSchemaValidator.ts`));
  schemas = actual.load(path.join(root, `blueprint/implementations/billing/stripe/domain/schemas/${flavor}/${kind}.schema.ts`))[`Stripe${title}ServiceSchemas`];
  const validator = statics.SchemaValidator();
  const mappers = actual.load(path.join(root, `blueprint/billing-stripe/domain/mappers/${kind}.mappers.ts`));
  const Service = actual.load(path.join(root, `blueprint/implementations/billing/stripe/services/${kind}.service.ts`))[`Stripe${title}Service`];
  const payloads = [], persisted = [], retrieved = [];
  const provider = kind === 'plan' ? { id: 'prod_owned', object: 'product', name: 'Owned' } : { id: 'sub_owned', object: 'subscription', customer: 'cus_owned' };
  const em = Object.assign(new EntityManager(), {
    create(_entity, fields) { return { id: randomUUID(), ...fields }; },
    async transactional(callback) { return callback(this); },
    async persist(entity) { persisted.push(structuredClone(entity)); }
  });
  const stripe = kind === 'plan' ? { plans: {
    async create(payload) { payloads.push(structuredClone(payload)); return { id: 'plan_owned', object: 'plan' }; },
    async retrieve(id, options) { retrieved.push(structuredClone({ id, options })); return { id, object: 'plan', product: provider }; }
  } } : { subscriptions: { async create(payload) { payloads.push(structuredClone(payload)); return provider; } } };
  const service = new Service(stripe, em, {}, validator, mappers);
  const base = kind === 'plan'
    ? { name: 'prod_owned', price: 1250, cadence: 'month', currency: 'USD', externalId: '', billingProvider: 'stripe', active: true }
    : { partyId: 'cus_owned', partyType: 'user', productId: 'plan_owned', active: true, externalId: '', startDate: new Date('2030-01-01T00:00:00Z'), status: 'active', billingProvider: 'stripe' };
  const schema = kind === 'plan' ? schemas.CreatePlanSchema : schemas.CreateSubscriptionSchema({ USER: 'user', ORGANIZATION: 'organization' });
  for (const fields of [undefined, {}, { metadata: { owner: 'owned' } }]) {
    const dto = { ...base, ...(fields === undefined ? {} : { stripeFields: fields }) };
    assert.equal(validator.parse(validator.schemify(schema), dto).ok, true);
    const made = await service[`create${title}`](dto);
    assert.deepEqual(made.stripeFields, provider);
    assert.equal(made.externalId, kind === 'plan' ? 'plan_owned' : 'sub_owned');
    assert.equal(made.id, persisted.at(-1).id);
    cases++;
  }
  const expected = kind === 'plan' ? { amount: 1250, interval: 'month', product: 'prod_owned', currency: 'USD' } : { customer: 'cus_owned', items: [{ plan: 'plan_owned' }] };
  assert.deepEqual(payloads[0], expected);
  assert.deepEqual(payloads[1], expected);
  assert.deepEqual(payloads[2], { metadata: { owner: 'owned' }, ...expected });
  assert.equal(persisted.length, 3);
  if (kind === 'plan') assert.deepEqual(retrieved, Array.from({ length: 3 }, () => ({ id: 'plan_owned', options: { expand: ['product'] } })));
}
console.log(`PASS ${cases} actual Zod/TypeBox plan/subscription create-option cases through configured mappers and service layers`);

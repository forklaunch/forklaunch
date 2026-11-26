// Actual configured schemas/mappers, JSON-cache lifecycle and optional restricted PG roundtrip.
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { actualValidator } from './rust_actual_validator_fixture.mjs';
import { betterAuthOrm } from './fixtures/rust_better_auth_orm.mjs';
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..'
);
const sample = {
  customerId: 'cus_owned',
  paymentMethods: ['card'],
  currency: 'USD',
  expiresAt: new Date('2030-01-01T00:00:00Z'),
  status: 'PENDING'
};
const provider = {
  id: 'cs_owned',
  object: 'checkout.session',
  mode: 'subscription',
  livemode: false,
  customer: 'cus_owned',
  url: null
};
let cases = 0;
for (const flavor of ['zod', 'typebox']) {
  let actual, statics, schemas;
  class EntityManager {}
  actual = actualValidator(root, (spec, file, load) => {
    if (spec === `@forklaunch/validator/${flavor}`) return statics;
    if (spec === '@forklaunch/blueprint-core')
      return { schemaValidator: statics.SchemaValidator() };
    if (spec === '@forklaunch/common')
      return {
        ...load(
          path.join(root, 'framework/common/src/deepCloneWithoutUndefined.ts')
        ),
        ...load(path.join(root, 'framework/common/src/InMemoryBlob.ts'))
      };
    if (spec === '@forklaunch/core/mappers')
      return load(path.join(root, 'framework/core/src/mappers/mapper.ts'));
    if (spec === '../schemas' && file.endsWith('/checkoutSession.mappers.ts'))
      return { CheckoutSessionSchemas: schemas };
    if (spec.includes('/persistence/entities/checkoutSession.entity'))
      return { CheckoutSession: { name: 'CheckoutSession' } };
    if (spec === '@mikro-orm/core') return { EntityManager };
    if (spec === '@forklaunch/core/cache')
      return { createCacheKey: (prefix) => (id) => prefix + ':' + id };
    if (spec === '@forklaunch/core/http')
      return { evaluateTelemetryOptions: () => ({ enabled: {} }) };
    if (spec === '@forklaunch/implementation-billing-base/services')
      return load(
        path.join(
          root,
          'blueprint/implementations/billing/base/services/checkoutSession.service.ts'
        )
      );
  });
  statics = actual.load(
    path.join(
      root,
      `framework/validator/src/${flavor}/staticSchemaValidator.ts`
    )
  );
  schemas = actual.load(
    path.join(
      root,
      `blueprint/implementations/billing/stripe/domain/schemas/${flavor}/checkoutSession.schema.ts`
    )
  ).StripeCheckoutSessionServiceSchemas;
  const validator = statics.SchemaValidator(),
    status = actual.load(
      path.join(root, 'blueprint/billing-stripe/domain/enum/status.enum.ts')
    ).StatusEnum;
  for (const factory of [
    schemas.CreateCheckoutSessionSchema,
    schemas.CheckoutSessionSchema
  ]) {
    const schema = validator.schemify(factory(status));
    for (const uri of [undefined, 'https://owned.test/session']) {
      assert.equal(
        validator.parse(schema, {
          ...sample,
          id: randomUUID(),
          uri,
          stripeFields: provider
        }).ok,
        true
      );
      cases++;
    }
  }
  const mappers = actual.load(
    path.join(
      root,
      'blueprint/billing-stripe/domain/mappers/checkoutSession.mappers.ts'
    )
  );
  const { StripeCheckoutSessionService } = actual.load(
    path.join(
      root,
      'blueprint/implementations/billing/stripe/services/checkoutSession.service.ts'
    )
  );
  const em = Object.assign(new EntityManager(), {
    create(_entity, fields) {
      return { id: randomUUID(), ...fields };
    },
    persist() {
      return { async flush() {} };
    }
  });
  // Optional create options must have the same provider payload as an empty object.
  for (const backup of [false, true]) {
    const payloads = [];
    const service = new StripeCheckoutSessionService(
      {
        checkout: {
          sessions: {
            async create(payload) {
              payloads.push(structuredClone(payload));
              return provider;
            }
          }
        }
      },
      em,
      { getTtlMilliseconds: () => 60000, async putRecord() {} },
      {},
      validator,
      mappers,
      { enableDatabaseBackup: backup }
    );
    for (const fields of [
      undefined,
      {},
      { customer: 'cus_explicit', metadata: { owned: 'kept' } }
    ]) {
      const dto = {
        ...sample,
        customerId: 'local-public-customer',
        ...(fields === undefined ? {} : { stripeFields: fields })
      };
      assert.equal(
        validator.parse(
          validator.schemify(schemas.CreateCheckoutSessionSchema(status)),
          dto
        ).ok,
        true
      );
      const made = await service.createCheckoutSession(dto);
      assert.equal(made.customerId, 'local-public-customer');
      assert.deepEqual(made.stripeFields, provider);
      cases++;
    }
    assert.deepEqual(payloads[0], payloads[1]);
    assert.deepEqual(payloads[0], {
      mode: 'subscription',
      payment_method_types: ['card'],
      currency: 'USD',
      success_url: undefined,
      cancel_url: undefined
    });
    assert.deepEqual(payloads[2], {
      ...payloads[0],
      customer: 'cus_explicit',
      metadata: { owned: 'kept' }
    });
  }
  for (const backup of [false, true])
    for (const url of [null, undefined, 'https://owned.test/session']) {
      const rows = new Map(),
        cache = {
          getTtlMilliseconds: () => 60000,
          async putRecord({ key, value }) {
            rows.set(key, JSON.stringify(value));
          },
          async readRecord(key) {
            return rows.has(key) ? { value: JSON.parse(rows.get(key)) } : null;
          }
        };
      const session = { ...provider, url },
        calls = [];
      const service = new StripeCheckoutSessionService(
        {
          checkout: {
            sessions: {
              async create() {
                return session;
              },
              async retrieve(id) {
                calls.push(id);
                return session;
              }
            }
          }
        },
        em,
        cache,
        {},
        validator,
        mappers,
        { enableDatabaseBackup: backup }
      );
      const made = await service.createCheckoutSession({
        ...sample,
        stripeFields: {}
      });
      assert.equal(made.uri, url ?? undefined);
      const got = await service.getCheckoutSession({ id: made.id });
      assert.equal(got.uri, url ?? undefined);
      assert.equal(got.id, made.id);
      assert.equal(got.customerId, sample.customerId);
      assert.deepEqual(calls, ['cs_owned']);
      cases++;
      const hydrated = await mappers.CheckoutSessionMapper.toDto({
        ...made,
        uri: null,
        providerFields: session
      });
      assert.equal(hydrated.uri, undefined);
      cases++;
    }
}
console.log(
  `PASS ${cases} actual Zod/TypeBox configured checkout optional-URI/provider-options/schema/mapper/cache cases`
);
if (!process.env.FORKLAUNCH_TEST_POSTGRES_URL) {
  console.log(
    'SKIP restricted PostgreSQL nullable checkout roundtrip: set FORKLAUNCH_TEST_POSTGRES_URL'
  );
  process.exit(0);
}
const source = await betterAuthOrm(
  root,
  process.env.FORKLAUNCH_TEST_ORM_DEPENDENCIES,
  (spec, _file, load) => {
    if (spec === '@forklaunch/implementation-billing-stripe/enum')
      return {
        ...load(
          path.join(
            root,
            'blueprint/implementations/billing/stripe/domain/enum/currency.enum.ts'
          )
        ),
        ...load(
          path.join(
            root,
            'blueprint/implementations/billing/stripe/domain/enum/paymentMethod.enum.ts'
          )
        )
      };
  },
  true,
  'blueprint/billing-stripe/persistence/entities/checkoutSession.entity.ts'
);
const suffix = randomUUID().replaceAll('-', ''),
  schema = `fl_checkout_uri_${suffix}`,
  role = `fl_uri_${suffix}`;
const owner = await source.MikroORM.init({
  entities: Object.values(source.entities),
  clientUrl: process.env.FORKLAUNCH_TEST_POSTGRES_URL,
  schema
});
let runtime;
try {
  await owner.schema.create();
  const conn = owner.em.getConnection();
  await conn.execute(
    `create role ${role} login nosuperuser nobypassrls password 'owned-uri-fixture'`
  );
  await conn.execute(`grant usage on schema ${schema} to ${role}`);
  await conn.execute(
    `grant select,insert,update,delete on all tables in schema ${schema} to ${role}`
  );
  const url = new URL(process.env.FORKLAUNCH_TEST_POSTGRES_URL);
  url.username = role;
  url.password = 'owned-uri-fixture';
  runtime = await source.MikroORM.init({
    entities: Object.values(source.entities),
    clientUrl: url.href,
    schema
  });
  const em = runtime.em.fork(),
    Entity = source.entities.CheckoutSession;
  const row = em.create(Entity, {
    ...sample,
    uri: null,
    successRedirectUri: null,
    cancelRedirectUri: null,
    providerFields: provider
  });
  em.persist(row);
  await em.flush();
  em.clear();
  const read = await em.findOneOrFail(Entity, { id: row.id });
  assert.equal(read.uri, null);
  assert.equal(read.customerId, sample.customerId);
  const rows = await conn.execute(
    `select uri from ${schema}.checkout_session where id = ?`,
    [row.id]
  );
  assert.equal(rows[0].uri, null);
  // An existing NOT NULL deployment must migrate before accepting missing URLs.
  await em.nativeDelete(Entity, { id: row.id });
  await conn.execute(
    `alter table ${schema}.checkout_session alter column uri set not null`
  );
  const old = runtime.em.fork();
  const rejected = old.create(Entity, {
    ...sample,
    uri: null,
    providerFields: provider
  });
  old.persist(rejected);
  await assert.rejects(old.flush());
  await conn.execute(
    `alter table ${schema}.checkout_session alter column uri drop not null`
  );
  const upgraded = runtime.em.fork();
  const accepted = upgraded.create(Entity, {
    ...sample,
    uri: null,
    providerFields: provider
  });
  upgraded.persist(accepted);
  await upgraded.flush();
  assert.equal(
    (await runtime.em.fork().findOneOrFail(Entity, { id: accepted.id })).uri,
    null
  );
  console.log(
    'PASS actual configured entity + MikroORM restricted PostgreSQL NULL roundtrip and existing NOT NULL upgrade'
  );
} finally {
  if (runtime) await runtime.close(true);
  await owner.em
    .getConnection()
    .execute(`drop schema if exists ${schema} cascade`);
  await owner.em.getConnection().execute(`drop owned by ${role}`);
  await owner.em.getConnection().execute(`drop role if exists ${role}`);
  await owner.close(true);
}

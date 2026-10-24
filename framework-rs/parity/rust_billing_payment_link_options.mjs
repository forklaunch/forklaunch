// Actual dual-validator schemas and configured service dispatch; no live provider.
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { actualValidator } from './rust_actual_validator_fixture.mjs';
const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..'
);
const invalid = [
  undefined,
  null,
  false,
  1,
  'bad',
  [],
  {},
  { line_items: null },
  { line_items: {} },
  { line_items: 'bad' },
  { line_items: [null] },
  { line_items: [1] },
  { line_items: [[]] }
];
let cases = 0;
for (const flavor of ['zod', 'typebox']) {
  let statics, schemas;
  class EntityManager {}
  const actual = actualValidator(root, (spec, file, load) => {
    if (spec === `@forklaunch/validator/${flavor}`) return statics;
    if (spec === '@forklaunch/blueprint-core')
      return { schemaValidator: statics.SchemaValidator() };
    if (spec === '@forklaunch/core/mappers')
      return load(path.join(root, 'framework/core/src/mappers/mapper.ts'));
    if (spec === '../schemas' && file.endsWith('/paymentLink.mappers.ts'))
      return { PaymentLinkSchemas: schemas };
    if (spec.includes('/persistence/entities/paymentLink.entity'))
      return { PaymentLink: { name: 'PaymentLink' } };
    if (spec === '@mikro-orm/core') return { EntityManager };
    if (spec === '@forklaunch/core/cache')
      return { createCacheKey: (prefix) => (id) => prefix + ':' + id };
    if (spec === '@forklaunch/core/http')
      return { evaluateTelemetryOptions: () => ({ enabled: {} }) };
    if (spec === '@forklaunch/implementation-billing-base/services')
      return load(
        path.join(
          root,
          'blueprint/implementations/billing/base/services/paymentLink.service.ts'
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
      `blueprint/implementations/billing/stripe/domain/schemas/${flavor}/paymentLink.schema.ts`
    )
  ).StripePaymentLinkServiceSchemas;
  const validator = statics.SchemaValidator(),
    status = actual.load(
      path.join(root, 'blueprint/billing-stripe/domain/enum/status.enum.ts')
    ).StatusEnum;
  const schema = validator.schemify(schemas.CreatePaymentLinkSchema(status));
  const mappers = actual.load(
    path.join(
      root,
      'blueprint/billing-stripe/domain/mappers/paymentLink.mappers.ts'
    )
  );
  const { StripePaymentLinkService } = actual.load(
    path.join(
      root,
      'blueprint/implementations/billing/stripe/services/paymentLink.service.ts'
    )
  );
  for (const backup of [false, true]) {
    let writes = 0,
      allocations = 0,
      persists = 0;
    const calls = [];
    const provider = {
      id: 'plink_owned',
      object: 'payment_link',
      livemode: false,
      active: true
    };
    const em = Object.assign(new EntityManager(), {
      create(_entity, fields) {
        allocations++;
        return { id: randomUUID(), ...fields };
      },
      persist() {
        persists++;
        return { async flush() {} };
      }
    });
    const service = new StripePaymentLinkService(
      {
        paymentLinks: {
          async create(payload) {
            calls.push(structuredClone(payload));
            return provider;
          }
        }
      },
      em,
      {
        getTtlMilliseconds: () => 60000,
        async putRecord() {
          writes++;
        }
      },
      {},
      validator,
      mappers,
      { enableDatabaseBackup: backup }
    );
    const sample = {
      amount: 100,
      currency: 'USD',
      paymentMethods: ['card'],
      status: 'PENDING'
    };
    for (const stripeFields of invalid) {
      const dto = {
        ...sample,
        ...(stripeFields === undefined ? {} : { stripeFields })
      };
      assert.equal(
        validator.parse(schema, dto).ok,
        false,
        flavor + JSON.stringify(stripeFields)
      );
      // Configured creation preflight (validateCreate) rejects first.
      await assert.rejects(
        service.createPaymentLink(dto),
        /Invalid paymentLink creation/
      );
      assert.equal(calls.length, 0);
      assert.equal(writes, 0);
      assert.equal(allocations, 0);
      cases++;
    }
    for (const stripeFields of [
      { line_items: [] },
      {
        line_items: [
          {
            price: 'price_owned',
            quantity: 1,
            extension: { untouched: [null, 'kept'] }
          }
        ],
        metadata: { key: 'value' },
        extension: { nested: { raw: [1, false, null] } }
      }
    ]) {
      const dto = { ...sample, stripeFields };
      const parsed = validator.parse(schema, dto);
      assert.equal(parsed.ok, true);
      assert.deepEqual(
        JSON.parse(JSON.stringify(parsed.value.stripeFields)),
        stripeFields
      );
      const result = await service.createPaymentLink(dto);
      assert.deepEqual(result.stripeFields, provider);
      assert.deepEqual(calls.at(-1), {
        ...stripeFields,
        payment_method_types: ['card'],
        currency: 'USD'
      });
      cases++;
    }
    assert.equal(persists, backup ? 2 : 0);
    assert.equal(writes, 2);
  }
}
console.log(
  `PASS ${cases} actual Zod/TypeBox required PaymentLink provider-object/line_items cases, pre-provider denials and exact nested option preservation`
);

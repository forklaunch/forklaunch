/**
 * Repro harness for the mikro-orm 7.1.x webhook crash: a consuming
 * application that discovers ONLY its own entities. The service's
 * idempotency lookup/insert must resolve by name against the app's
 * discovered `StripeWebhookEvent` — passing this package's internal entity
 * object made mikro-orm 7.1.x throw
 * `Cannot read properties of undefined (reading 'filter')` in
 * EntityLoader.lookupEagerLoadedRelationships for every webhook.
 */
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { MikroORM } from '@mikro-orm/sqlite';
import Stripe from 'stripe';
import { v4 } from 'uuid';
import { StripeWebhookService } from '../services/webhook.service';

// The application's own entity definition — mirrors the blueprint app's
// (sqlBaseProperties-style id) and is the ONLY entity the ORM discovers.
const StripeWebhookEvent = defineComplianceEntity({
  name: 'StripeWebhookEvent',
  properties: {
    id: fp
      .uuid()
      .primary()
      .onCreate(() => v4())
      .compliance('none'),
    stripeId: fp.string().compliance('none'),
    idempotencyKey: fp.string().nullable().compliance('none'),
    eventType: fp.string().compliance('none'),
    eventData: fp.json<unknown>().compliance('none')
  }
});

const noopOtel = {
  info: () => {},
  warn: () => {},
  error: () => {},
  debug: () => {}
};

describe('webhook idempotency against an app-discovered entity', () => {
  let orm: Awaited<ReturnType<typeof MikroORM.init>>;

  beforeAll(async () => {
    orm = await MikroORM.init({
      dbName: ':memory:',
      entities: [StripeWebhookEvent],
      allowGlobalContext: true
    });
    await orm.schema.create();
  });

  afterAll(async () => {
    await orm.close();
  });

  const makeService = () =>
    new StripeWebhookService(
      null as unknown as Stripe,
      orm.em.fork(),
      null as never,
      noopOtel as never,
      // sub-services are never reached for an unhandled event type
      null as never,
      null as never,
      null as never,
      null as never,
      null as never,
      { USER: 'user' } as never,
      StripeWebhookEvent
    );

  const event = {
    id: 'evt_test_1',
    type: 'some.unhandled.event',
    request: { idempotency_key: 'ik_test_1' },
    data: { object: {} }
  } as unknown as Stripe.Event;

  test('handles the event without the undiscovered-entity crash and writes one row', async () => {
    await makeService().handleWebhookEvent(event);

    const rows = await orm.em
      .fork()
      .find<{ idempotencyKey: string }>(
        'StripeWebhookEvent' as never,
        {} as never
      );
    expect(rows).toHaveLength(1);
    expect(rows[0].idempotencyKey).toBe('ik_test_1');
  });

  test('replaying the same Stripe event id is a no-op', async () => {
    await makeService().handleWebhookEvent(event);

    const rows = await orm.em
      .fork()
      .find('StripeWebhookEvent' as never, {} as never);
    expect(rows).toHaveLength(1);
  });

  test('an app can inject its own discovered entity object (mapper-style)', async () => {
    const service = new StripeWebhookService(
      null as unknown as Stripe,
      orm.em.fork(),
      null as never,
      noopOtel as never,
      null as never,
      null as never,
      null as never,
      null as never,
      null as never,
      { USER: 'user' } as never,
      StripeWebhookEvent
    );
    const injectedEvent = {
      id: 'evt_test_2',
      type: 'some.unhandled.event',
      request: { idempotency_key: 'ik_test_2' },
      data: { object: {} }
    } as unknown as Stripe.Event;
    await service.handleWebhookEvent(injectedEvent);

    const rows = await orm.em
      .fork()
      .find('StripeWebhookEvent' as never, {} as never);
    expect(rows).toHaveLength(2);
  });
});

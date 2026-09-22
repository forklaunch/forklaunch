/**
 * Stripe subscription events carry Stripe's subscription id (`sub_…`), which
 * is our `externalId` and never our primary key. Before this, the updated /
 * deleted / paused / resumed handlers passed that id straight to
 * `getSubscription({ id })`, which cannot match a uuid primary key, so every
 * one of those events failed. Idempotency also keyed on
 * `request.idempotency_key`, which Stripe leaves null for the events it
 * originates itself — and `{ idempotencyKey: null }` matched whichever row
 * had none, swallowing every later event.
 */
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import { MikroORM } from '@mikro-orm/sqlite';
import Stripe from 'stripe';
import { v4 } from 'uuid';
import { StripeWebhookService } from '../services/webhook.service';

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

type Call = { method: string; args: unknown[] };

/** A recording stand-in for the subscription service's base half. */
function fakeSubscriptionService(
  existing: { id: string } | null,
  calls: Call[]
) {
  const record =
    (method: string) =>
    async (...args: unknown[]) => {
      calls.push({ method, args });
      return {};
    };
  return {
    baseSubscriptionService: {
      findSubscriptionIdByExternalId: async (dto: { externalId: string }) => {
        calls.push({ method: 'findSubscriptionIdByExternalId', args: [dto] });
        return existing;
      },
      createSubscription: record('createSubscription'),
      updateSubscription: record('updateSubscription'),
      deleteSubscription: record('deleteSubscription'),
      cancelSubscription: record('cancelSubscription'),
      resumeSubscription: record('resumeSubscription')
    }
  };
}

const subscriptionObject = (overrides: Record<string, unknown> = {}) => ({
  id: 'sub_123',
  customer: 'cus_123',
  status: 'active',
  created: 1_700_000_000,
  cancel_at: null,
  description: null,
  items: { data: [{ plan: { id: 'price_pro' } }] },
  ...overrides
});

const stripeEvent = (
  id: string,
  type: string,
  object: unknown,
  idempotencyKey: string | null = null
) =>
  ({
    id,
    type,
    request: { id: 'req_1', idempotency_key: idempotencyKey },
    data: { object }
  }) as unknown as Stripe.Event;

describe('Stripe subscription webhook events', () => {
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

  const makeService = (
    existing: { id: string } | null,
    calls: Call[] = []
  ) => ({
    calls,
    service: new StripeWebhookService(
      null as unknown as Stripe,
      orm.em.fork(),
      null as never,
      noopOtel as never,
      null as never,
      null as never,
      null as never,
      null as never,
      fakeSubscriptionService(existing, calls) as never,
      { USER: 'user', ORGANIZATION: 'organization' } as never,
      StripeWebhookEvent
    )
  });

  const recordedEvents = () =>
    orm.em
      .fork()
      .find<{ stripeId: string; idempotencyKey: string | null }>(
        'StripeWebhookEvent' as never,
        {} as never
      );

  test('two Stripe-originated events (null idempotency key) are both processed', async () => {
    const first = makeService({ id: 'row-1' });
    await first.service.handleWebhookEvent(
      stripeEvent(
        'evt_null_1',
        'customer.subscription.updated',
        subscriptionObject()
      )
    );
    const second = makeService({ id: 'row-1' });
    await second.service.handleWebhookEvent(
      stripeEvent(
        'evt_null_2',
        'customer.subscription.updated',
        subscriptionObject()
      )
    );

    expect(first.calls.map((c) => c.method)).toContain('updateSubscription');
    expect(second.calls.map((c) => c.method)).toContain('updateSubscription');
    const rows = await recordedEvents();
    expect(rows.map((r) => r.stripeId).sort()).toEqual([
      'evt_null_1',
      'evt_null_2'
    ]);
  });

  test('a redelivered event (same Stripe id) is a no-op', async () => {
    const replay = makeService({ id: 'row-1' });
    await replay.service.handleWebhookEvent(
      stripeEvent(
        'evt_null_1',
        'customer.subscription.updated',
        subscriptionObject()
      )
    );
    expect(replay.calls).toEqual([]);
    expect((await recordedEvents()).length).toBe(2);
  });

  test('updated resolves our row by externalId and leaves the party alone', async () => {
    const { service, calls } = makeService({ id: 'row-1' });
    await service.handleWebhookEvent(
      stripeEvent(
        'evt_upd',
        'customer.subscription.updated',
        subscriptionObject({ status: 'past_due' })
      )
    );
    const lookup = calls.find(
      (c) => c.method === 'findSubscriptionIdByExternalId'
    );
    expect(lookup?.args[0]).toEqual({ externalId: 'sub_123' });
    const update = calls.find((c) => c.method === 'updateSubscription');
    expect(update?.args[0]).toMatchObject({
      id: 'row-1',
      externalId: 'sub_123',
      productId: 'price_pro',
      status: 'past_due',
      active: false
    });
    expect(update?.args[0]).not.toHaveProperty('partyId');
    expect(update?.args[0]).not.toHaveProperty('partyType');
  });

  test('updated for a subscription we never recorded creates it', async () => {
    const { service, calls } = makeService(null);
    await service.handleWebhookEvent(
      stripeEvent(
        'evt_upd_new',
        'customer.subscription.updated',
        subscriptionObject()
      )
    );
    const create = calls.find((c) => c.method === 'createSubscription');
    expect(create?.args[0]).toMatchObject({
      externalId: 'sub_123',
      partyId: 'cus_123',
      partyType: 'user',
      active: true
    });
    expect(calls.map((c) => c.method)).not.toContain('updateSubscription');
  });

  test.each([
    ['customer.subscription.deleted', 'deleteSubscription'],
    ['customer.subscription.paused', 'cancelSubscription'],
    ['customer.subscription.resumed', 'resumeSubscription']
  ])('%s targets our row id', async (type, method) => {
    const { service, calls } = makeService({ id: 'row-9' });
    await service.handleWebhookEvent(
      stripeEvent(`evt_${method}`, type, subscriptionObject())
    );
    const call = calls.find((c) => c.method === method);
    expect(call?.args[0]).toEqual({ id: 'row-9' });
  });

  test('deleted for an unknown subscription is recorded and nothing else', async () => {
    const { service, calls } = makeService(null);
    await service.handleWebhookEvent(
      stripeEvent(
        'evt_del_unknown',
        'customer.subscription.deleted',
        subscriptionObject()
      )
    );
    expect(calls.map((c) => c.method)).toEqual([
      'findSubscriptionIdByExternalId'
    ]);
    expect(
      (await recordedEvents()).some((r) => r.stripeId === 'evt_del_unknown')
    ).toBe(true);
  });
});

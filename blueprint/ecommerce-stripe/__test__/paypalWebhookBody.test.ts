import { describe, expect, it } from 'vitest';
import { schemaValidator } from '../schema';
import { PaypalWebhookEventSchema } from '../domain/schemas/webhook.schema';

// PayPal signs transmission_id|transmission_time|webhook_id|CRC32(body) and
// its docs require the event be posted back to verify-webhook-signature
// "exactly as it was received, with no deviations in formatting or content of
// any kind". req.body cannot satisfy that, because the validator keeps only
// the fields the route schema declares. This pins that gap so nobody
// reintroduces it by passing req.body to verification: if this test starts
// failing because the fields survive, the raw-body handling can be revisited.
describe('PayPal webhook body validation', () => {
  const deliveredByPaypal = {
    id: 'WH-2WR32451HC0233532-67976317FL4543714',
    event_type: 'PAYMENT.CAPTURE.COMPLETED',
    create_time: '2026-08-24T00:00:00Z',
    event_version: '1.0',
    resource_type: 'capture',
    resource_version: '2.0',
    summary: 'Payment completed for $ 10.0 USD',
    links: [
      {
        href: 'https://api-m.paypal.com/v1/notifications/webhooks-events/WH-2WR',
        rel: 'self'
      }
    ],
    resource: {
      id: '8MC585209K746392H',
      supplementary_data: { related_ids: { order_id: '5O190127TN364715T' } }
    }
  };

  it('drops fields the schema does not declare, so req.body is not the delivered event', () => {
    const result = schemaValidator.parse(
      PaypalWebhookEventSchema,
      deliveredByPaypal
    );
    expect(result.ok).toBe(true);

    const validated = (result as { ok: true; value: Record<string, unknown> })
      .value;

    for (const dropped of [
      'create_time',
      'event_version',
      'resource_type',
      'resource_version',
      'summary',
      'links'
    ]) {
      expect(validated).not.toHaveProperty(dropped);
    }
  });

  it('parsing the raw bytes preserves the delivered event in full', () => {
    const rawBody = Buffer.from(JSON.stringify(deliveredByPaypal), 'utf8');
    expect(JSON.parse(rawBody.toString('utf8'))).toEqual(deliveredByPaypal);
  });
});

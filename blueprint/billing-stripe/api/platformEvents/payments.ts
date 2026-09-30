import type { PlatformEvent } from '@forklaunch/core/http';
import type Stripe from 'stripe';
import { ci, tokens } from '../../bootstrapper';

const webhookServiceFactory = ci.scopedResolver(tokens.WebhookService);

/**
 * Stripe events for this instance's connected account, relayed by the
 * ForkLaunch platform after it verified Stripe's signature (`forklaunch infra
 * add <service> payments`). They go to the same StripeWebhookService the
 * key-verified /webhook route uses outside managed mode.
 *
 *   event.id    the Stripe event id (stable across redeliveries)
 *   event.type  the Stripe event type
 *   event.data  the event's object
 *
 * Deliveries can repeat: dedupe on event.id. The in-memory set below only
 * covers one process; record processed ids in the database for more.
 */
const processed = new Set<string>();

export async function handle(event: PlatformEvent): Promise<void> {
  if (processed.has(event.id)) return;
  await webhookServiceFactory().handleWebhookEvent({
    id: event.id,
    object: 'event',
    type: event.type,
    created: Math.floor(new Date(event.occurredAt).getTime() / 1000),
    data: { object: event.data }
  } as unknown as Stripe.Event);
  // TODO: anything else this app records about a payment.
  processed.add(event.id);
}

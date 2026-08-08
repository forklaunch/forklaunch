import {
  WorkerFailureHandler,
  WorkerProcessFunction
} from '@forklaunch/interfaces-worker/types';
import { ci, tokens } from './bootstrapper';
import { OrderStatus } from '@forklaunch/interfaces-ecommerce/types';
import type { OrderEventRecord } from './persistence/entities/orderEvent.entity';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const inventoryServiceFactory = ci.scopedResolver(tokens.InventoryService);

/**
 * ECOM-05/12's inventory side-effect, actually built — the module's real
 * gap this session (search/filter, checkout, PayPal, cart caching) closed
 * but the worker never had. Deliberately scoped to two side effects that
 * are provable against the real database — decrement on paid, restock on
 * cancelled — not shipping/invoicing/notifications, which need external
 * services this environment has no credentials for.
 */
const processOrderEvents: WorkerProcessFunction<OrderEventRecord> = async (
  events
) => {
  const failedEvents: { value: OrderEventRecord; error: Error }[] = [];

  for (const event of events) {
    try {
      openTelemetryCollector.info(
        `Processing order event: ${event.orderId} ${event.fromStatus} -> ${event.toStatus}`
      );

      // A fresh scoped resolution per event, not one shared instance across
      // the whole batch — same reason the HTTP handlers use ci.scopedResolver
      // per request rather than a module-level singleton.
      const inventoryService = inventoryServiceFactory();

      // All items in one transaction, so the adjustment is all-or-nothing.
      // Without this the loop had no per-item checkpoint: if item 2 threw
      // (oversell guard, transient DB error), item 1's decrement had already
      // committed, the whole event was requeued unchanged, and the retry
      // decremented item 1 a second time — stock permanently wrong, then
      // silently dropped once retryCount exceeded the consumer's limit.
      // Rolling back on failure means the retry starts from a clean slate.
      const applyStockDeltas = async (sign: 1 | -1) => {
        await inventoryService.em.transactional(async (innerEm) => {
          for (const item of event.items) {
            await inventoryService.adjustStock(
              {
                variantId: item.variantId,
                delta: sign * item.quantity
              },
              innerEm
            );
          }
        });
      };

      if (event.toStatus === OrderStatus.PAID) {
        await applyStockDeltas(-1);
      } else if (
        event.toStatus === OrderStatus.CANCELLED &&
        // Restock only if stock was actually decremented earlier — that
        // happens exactly once, at PENDING -> PAID. So restock on
        // cancellation from any state that implies stock was already
        // taken: PAID (ORDER_TRANSITIONS in order.service.ts allows
        // PAID -> CANCELLED directly) or FULFILLED (FULFILLED -> CANCELLED
        // is also legal — this order already passed through PAID to get
        // there, so stock was decremented then and was never restocked in
        // between). SHIPPED and DELIVERED are excluded not because stock
        // wasn't taken (it was), but because ORDER_TRANSITIONS doesn't
        // allow cancelling from either state — this branch can never
        // observe fromStatus SHIPPED/DELIVERED in practice. Cancelling
        // directly from PENDING never touched stock (found by testing: a
        // pending->cancelled order was blindly restocking anyway,
        // inflating stock for something that was never removed), so it's
        // still deliberately excluded here.
        (event.fromStatus === OrderStatus.PAID ||
          event.fromStatus === OrderStatus.FULFILLED)
      ) {
        await applyStockDeltas(1);
      }

      event.processed = true;
    } catch (error) {
      failedEvents.push({ value: event, error: error as Error });
    }
  }

  return failedEvents;
};

const processFailures: WorkerFailureHandler<OrderEventRecord> = async (
  events
) => {
  events.forEach((event) => {
    openTelemetryCollector.error(
      'Order event processing failed',
      event.error,
      event.value
    );
  });
};

const orderEventConsumerFactory = ci.resolve(tokens.OrderEventConsumer);
const consumer = orderEventConsumerFactory(processOrderEvents, processFailures);

consumer.start();
openTelemetryCollector.info('Ecommerce worker started, consuming order-events queue');

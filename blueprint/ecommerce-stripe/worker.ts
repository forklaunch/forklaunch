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

      if (event.toStatus === OrderStatus.PAID) {
        for (const item of event.items) {
          await inventoryService.adjustStock({
            variantId: item.variantId,
            delta: -item.quantity
          });
        }
      } else if (
        event.toStatus === OrderStatus.CANCELLED &&
        event.fromStatus === OrderStatus.PAID
      ) {
        // Only restock if inventory was actually decremented earlier —
        // cancelling directly from pending never touched stock (found by
        // testing: a pending->cancelled order was blindly restocking
        // anyway, inflating stock for something that was never removed).
        for (const item of event.items) {
          await inventoryService.adjustStock({
            variantId: item.variantId,
            delta: item.quantity
          });
        }
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

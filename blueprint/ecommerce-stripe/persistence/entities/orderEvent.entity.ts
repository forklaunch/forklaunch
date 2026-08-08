import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { OrderItemDto, OrderStatusType } from '@forklaunch/interfaces-ecommerce/types';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';

/**
 * The ECOM-12 event-emission boundary, finally implemented — previously
 * just a comment in order.service.ts. One row per order transition,
 * consumed by worker.ts to drive side effects (inventory adjustment today;
 * shipping/invoices/notifications are future consumers of the same event).
 */
export const OrderEventRecord = defineComplianceEntity({
  name: 'OrderEventRecord',
  properties: {
    ...sqlBaseProperties,
    orderId: fp.string().compliance('none'),
    fromStatus: fp.string().compliance('none'),
    toStatus: fp.string().compliance('none'),
    items: fp.json<OrderItemDto[]>().compliance('none'),
    processed: fp.boolean().compliance('none'),
    retryCount: fp.integer().compliance('none')
  }
});

export type OrderEventRecord = InferEntity<typeof OrderEventRecord> & {
  fromStatus: OrderStatusType;
  toStatus: OrderStatusType;
};

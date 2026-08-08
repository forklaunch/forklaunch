import { IdsDto } from '@forklaunch/common';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { OrderService } from '@forklaunch/interfaces-ecommerce/interfaces';
import {
  CreateOrderDto,
  OrderStatus,
  OrderStatusType,
  TransitionOrderDto
} from '@forklaunch/interfaces-ecommerce/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseOrderDtos } from '../domain/types/baseEcommerceDto.types';
import { BaseOrderEntities } from '../domain/types/baseEcommerceEntity.types';
import { OrderMappers } from '../domain/types/order.mapper.types';
import { Order } from '../persistence/entities';

/**
 * The ECOM-07 order state machine — the legal-transition table. Cancellation
 * is legal from any pre-terminal state; delivered/cancelled are terminal.
 */
const ORDER_TRANSITIONS: Record<OrderStatusType, OrderStatusType[]> = {
  [OrderStatus.PENDING]: [OrderStatus.PAID, OrderStatus.CANCELLED],
  [OrderStatus.PAID]: [OrderStatus.FULFILLED, OrderStatus.CANCELLED],
  [OrderStatus.FULFILLED]: [OrderStatus.SHIPPED, OrderStatus.CANCELLED],
  [OrderStatus.SHIPPED]: [OrderStatus.DELIVERED],
  [OrderStatus.DELIVERED]: [],
  [OrderStatus.CANCELLED]: []
};

export class IllegalOrderTransitionError extends Error {
  constructor(from: OrderStatusType, to: OrderStatusType) {
    super(`Illegal order transition: ${from} -> ${to}`);
    this.name = 'IllegalOrderTransitionError';
  }
}

export class BaseOrderService<
  SchemaValidator extends AnySchemaValidator,
  MapperEntities extends BaseOrderEntities,
  MapperDomains extends BaseOrderDtos = BaseOrderDtos
> implements OrderService
{
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: OrderMappers<MapperEntities, MapperDomains>;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: OrderMappers<MapperEntities, MapperDomains>,
    options?: {
      telemetry?: TelemetryOptions;
    }
  ) {
    this.em = em;
    this.openTelemetryCollector = openTelemetryCollector;
    this.schemaValidator = schemaValidator;
    this.mappers = mappers;
    this.evaluatedTelemetryOptions = options?.telemetry
      ? evaluateTelemetryOptions(options.telemetry).enabled
      : {
          logging: false,
          metrics: false,
          tracing: false
        };
  }

  async listOrders(
    idsDto?: IdsDto,
    em?: EntityManager
  ): Promise<MapperDomains['OrderMapper'][]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Listing orders', idsDto);
    }
    return Promise.all(
      (
        await (em ?? this.em).findAll(
          this.mappers.OrderMapper.entity as typeof Order,
          {
            where: idsDto?.ids?.length ? { id: { $in: idsDto.ids } } : undefined
          }
        )
      ).map((order) =>
        this.mappers.OrderMapper.toDto(
          order as InferEntity<MapperEntities['OrderMapper']>
        )
      )
    );
  }

  /** Status always defaults to pending on creation (ECOM-08). */
  async createOrder(
    orderDto: CreateOrderDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['OrderMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Creating order', orderDto);
    }
    const order = await this.mappers.CreateOrderMapper.toEntity(
      orderDto,
      em ?? this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(order);
    });
    return this.mappers.OrderMapper.toDto(order);
  }

  async getOrder(
    idDto: { id: string },
    em?: EntityManager
  ): Promise<MapperDomains['OrderMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting order', idDto);
    }
    const order = await (em ?? this.em).findOneOrFail(
      this.mappers.OrderMapper.entity as typeof Order,
      idDto
    );
    return this.mappers.OrderMapper.toDto(
      order as InferEntity<MapperEntities['OrderMapper']>
    );
  }

  /**
   * Rejects illegal transitions per ORDER_TRANSITIONS. Every legal transition
   * is a single persisted state change — the event-emission-to-worker
   * boundary (ECOM-12) hooks in at the deployable-app layer, one enqueue per
   * transition.
   */
  async transitionOrder(
    transitionDto: TransitionOrderDto,
    em?: EntityManager
  ): Promise<MapperDomains['OrderMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Transitioning order', transitionDto);
    }
    const entityManager = em ?? this.em;
    const entity = this.mappers.OrderMapper.entity as typeof Order;
    const order = await entityManager.findOneOrFail(entity, {
      id: transitionDto.id
    });
    const currentStatus = order.status as OrderStatusType;
    const legalNextStates = ORDER_TRANSITIONS[currentStatus] ?? [];
    if (!legalNextStates.includes(transitionDto.to)) {
      throw new IllegalOrderTransitionError(currentStatus, transitionDto.to);
    }

    // Atomic conditional transition, same nativeUpdate idiom as
    // Inventory.adjustStock. The status we validated against goes in the
    // WHERE clause, so the check and the write cannot be split by a
    // concurrent transition. Read-then-write here let two callers both read
    // PENDING, both pass the legality check, and both persist — which made
    // order.controller.ts enqueue two order events for one order, and the
    // worker decrement inventory twice. Webhook retries make that a routine
    // occurrence, not a rare race.
    const affected = await entityManager.nativeUpdate(
      entity,
      { id: transitionDto.id, status: currentStatus },
      { status: transitionDto.to }
    );

    if (affected === 0) {
      // Someone else transitioned this order between our read and our write.
      // Re-read to report the status it actually landed in.
      const current = await entityManager.findOneOrFail(entity, {
        id: transitionDto.id
      });
      throw new IllegalOrderTransitionError(
        current.status as OrderStatusType,
        transitionDto.to
      );
    }

    entityManager.assign(order, { status: transitionDto.to });
    return this.mappers.OrderMapper.toDto(
      order as InferEntity<MapperEntities['OrderMapper']>
    );
  }
}

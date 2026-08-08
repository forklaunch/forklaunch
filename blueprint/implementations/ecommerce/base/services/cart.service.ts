import { TtlCache } from '@forklaunch/core/cache';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { CartService } from '@forklaunch/interfaces-ecommerce/interfaces';
import {
  AddCartItemDto,
  CreateCartDto,
  RemoveCartItemDto
} from '@forklaunch/interfaces-ecommerce/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseCartDtos } from '../domain/types/baseEcommerceDto.types';
import { BaseCartEntities } from '../domain/types/baseEcommerceEntity.types';
import { CartMappers } from '../domain/types/cart.mapper.types';
import { Cart } from '../persistence/entities';

const cartCacheKey = (id: string) => `cart:${id}`;

/** Cache calls must NEVER hang the caller — a try/catch alone doesn't
 *  protect against this: a promise that never settles (which the
 *  underlying Redis client can do when unreachable, confirmed by hand)
 *  isn't caught by a try/catch, it just blocks forever. A cache being
 *  "fast/temporary state" is only true if a broken cache can't turn into
 *  the slowest possible path through the code. */
const CACHE_TIMEOUT_MS = 750;
function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) =>
      setTimeout(() => reject(new Error(`Cache operation timed out after ${ms}ms`)), ms)
    )
  ]);
}

export class BaseCartService<
  SchemaValidator extends AnySchemaValidator,
  MapperEntities extends BaseCartEntities,
  MapperDomains extends BaseCartDtos = BaseCartDtos
> implements CartService
{
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: CartMappers<MapperEntities, MapperDomains>;
  protected cache?: TtlCache;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: CartMappers<MapperEntities, MapperDomains>,
    options?: {
      telemetry?: TelemetryOptions;
      /**
       * Optional — carts are "fast/temporary state" per the module's
       * design (Redis-backed read-through cache in front of Postgres,
       * which remains the source of truth). Undefined means Postgres-only
       * behavior, unchanged from before this cache existed.
       */
      cache?: TtlCache;
    }
  ) {
    this.em = em;
    this.openTelemetryCollector = openTelemetryCollector;
    this.schemaValidator = schemaValidator;
    this.mappers = mappers;
    this.cache = options?.cache;
    this.evaluatedTelemetryOptions = options?.telemetry
      ? evaluateTelemetryOptions(options.telemetry).enabled
      : {
          logging: false,
          metrics: false,
          tracing: false
        };
  }

  /**
   * Best-effort cache warm — never allowed to fail the caller. Redis is
   * explicitly temporary/fast state here, not the source of truth, so a
   * cache-write failure just means the next read falls back to Postgres,
   * not a broken request.
   */
  private async warmCache(dto: MapperDomains['CartMapper']): Promise<void> {
    if (!this.cache) return;
    try {
      await withTimeout(
        this.cache.putRecord({
          key: cartCacheKey((dto as { id: string }).id),
          value: dto,
          ttlMilliseconds: this.cache.getTtlMilliseconds()
        }),
        CACHE_TIMEOUT_MS
      );
    } catch (error) {
      this.openTelemetryCollector.error('Cart cache write failed', {
        error
      });
    }
  }

  async createCart(
    cartDto: CreateCartDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['CartMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Creating cart', cartDto);
    }
    const cart = await this.mappers.CreateCartMapper.toEntity(
      cartDto,
      em ?? this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(cart);
    });
    const dto = await this.mappers.CartMapper.toDto(cart);
    await this.warmCache(dto);
    return dto;
  }

  /** Read-through: cache first, Postgres (source of truth) on miss or cache failure. */
  async getCart(
    idDto: { id: string },
    em?: EntityManager
  ): Promise<MapperDomains['CartMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting cart', idDto);
    }

    if (this.cache) {
      try {
        const cached = await withTimeout(
          this.cache.readRecord<MapperDomains['CartMapper']>(
            cartCacheKey(idDto.id)
          ),
          CACHE_TIMEOUT_MS
        );
        return cached.value;
      } catch {
        // Cache miss, cache-read failure, or timeout — fall through to Postgres.
      }
    }

    const cart = await (em ?? this.em).findOneOrFail(
      this.mappers.CartMapper.entity as typeof Cart,
      idDto
    );
    const dto = await this.mappers.CartMapper.toDto(
      cart as InferEntity<MapperEntities['CartMapper']>
    );
    await this.warmCache(dto);
    return dto;
  }

  /**
   * Out-of-stock items are allowed in the cart — the check happens at
   * checkout, not here (per ECOM-06).
   */
  async addItem(
    addItemDto: AddCartItemDto,
    em?: EntityManager
  ): Promise<MapperDomains['CartMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Adding cart item', addItemDto);
    }
    // The schema layer types quantity as a bare number (the codebase-wide
    // convention), so nothing upstream rejects zero, negatives or fractions.
    // That matters here because cart quantities flow straight into checkout
    // pricing and the worker's stock deltas: a negative quantity drives the
    // order subtotal down (and the final Math.max(0, ...) clamp turns a
    // crafted cart into a ~$0 total), and flips the worker's `-quantity`
    // into a positive delta that inflates stock for goods never removed.
    // Enforce the invariant here, where it holds regardless of transport.
    if (!Number.isInteger(addItemDto.quantity) || addItemDto.quantity < 1) {
      throw new Error(
        `Cart item quantity must be a positive integer, received ${addItemDto.quantity}`
      );
    }

    const entityManager = em ?? this.em;
    const cart = await entityManager.findOneOrFail(
      this.mappers.CartMapper.entity as typeof Cart,
      { id: addItemDto.cartId }
    );
    const items = [...(cart.items ?? [])];
    const existing = items.find((i) => i.variantId === addItemDto.variantId);
    if (existing) {
      existing.quantity += addItemDto.quantity;
    } else {
      items.push({
        variantId: addItemDto.variantId,
        quantity: addItemDto.quantity
      });
    }
    entityManager.assign(cart, { items });
    await entityManager.transactional(async (innerEm) => {
      await innerEm.persist(cart);
    });
    const dto = await this.mappers.CartMapper.toDto(
      cart as InferEntity<MapperEntities['CartMapper']>
    );
    await this.warmCache(dto);
    return dto;
  }

  async removeItem(
    removeItemDto: RemoveCartItemDto,
    em?: EntityManager
  ): Promise<MapperDomains['CartMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Removing cart item', removeItemDto);
    }
    const entityManager = em ?? this.em;
    const cart = await entityManager.findOneOrFail(
      this.mappers.CartMapper.entity as typeof Cart,
      { id: removeItemDto.cartId }
    );
    const items = (cart.items ?? []).filter(
      (i) => i.variantId !== removeItemDto.variantId
    );
    entityManager.assign(cart, { items });
    await entityManager.transactional(async (innerEm) => {
      await innerEm.persist(cart);
    });
    const dto = await this.mappers.CartMapper.toDto(
      cart as InferEntity<MapperEntities['CartMapper']>
    );
    await this.warmCache(dto);
    return dto;
  }

  async clearCart(idDto: { id: string }, em?: EntityManager): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Clearing cart', idDto);
    }
    const entityManager = em ?? this.em;
    const cart = await entityManager.findOneOrFail(
      this.mappers.CartMapper.entity as typeof Cart,
      idDto
    );
    entityManager.assign(cart, { items: [] });
    await entityManager.transactional(async (innerEm) => {
      await innerEm.persist(cart);
    });
    await this.warmCache(
      await this.mappers.CartMapper.toDto(
        cart as InferEntity<MapperEntities['CartMapper']>
      )
    );
  }
}

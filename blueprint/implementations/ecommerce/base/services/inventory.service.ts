import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { InventoryService } from '@forklaunch/interfaces-ecommerce/interfaces';
import {
  AdjustStockDto,
  CreateInventoryDto,
  StockCheckDto,
  StockCheckResultDto
} from '@forklaunch/interfaces-ecommerce/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager, InferEntity, raw } from '@mikro-orm/core';
import { BaseInventoryDtos } from '../domain/types/baseEcommerceDto.types';
import { BaseInventoryEntities } from '../domain/types/baseEcommerceEntity.types';
import { InventoryMappers } from '../domain/types/inventory.mapper.types';
import { Inventory } from '../persistence/entities';

export class BaseInventoryService<
  SchemaValidator extends AnySchemaValidator,
  MapperEntities extends BaseInventoryEntities,
  MapperDomains extends BaseInventoryDtos = BaseInventoryDtos
> implements InventoryService
{
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: InventoryMappers<MapperEntities, MapperDomains>;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: InventoryMappers<MapperEntities, MapperDomains>,
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

  async createInventory(
    inventoryDto: CreateInventoryDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['InventoryMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Creating inventory', inventoryDto);
    }
    const inventory = await this.mappers.CreateInventoryMapper.toEntity(
      inventoryDto,
      em ?? this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(inventory);
    });
    return this.mappers.InventoryMapper.toDto(inventory);
  }

  async getInventory(
    variantIdDto: { variantId: string },
    em?: EntityManager
  ): Promise<MapperDomains['InventoryMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting inventory', variantIdDto);
    }
    const inventory = await (em ?? this.em).findOneOrFail(
      this.mappers.InventoryMapper.entity as typeof Inventory,
      { variantId: variantIdDto.variantId }
    );
    return this.mappers.InventoryMapper.toDto(
      inventory as InferEntity<MapperEntities['InventoryMapper']>
    );
  }

  async adjustStock(
    adjustDto: AdjustStockDto,
    em?: EntityManager
  ): Promise<MapperDomains['InventoryMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Adjusting stock', adjustDto);
    }
    const entityManager = em ?? this.em;
    const entity = this.mappers.InventoryMapper.entity as typeof Inventory;

    // Atomic conditional adjustment — the decrement/oversell guard lives in
    // the WHERE clause, not in app memory, so two concurrent orders for the
    // last unit can't both pass a stale read. Guild's commerce-security guide
    // is explicit this must never be a read-then-write across an await; this
    // is the same nativeUpdate+raw idiom used in gift-card/promo redemption.
    // A decrement (delta < 0) only applies where stock still covers it; a
    // restock (delta >= 0) is always safe, so no guard is needed.
    const where =
      adjustDto.delta < 0
        ? { variantId: adjustDto.variantId, stock: { $gte: -adjustDto.delta } }
        : { variantId: adjustDto.variantId };
    const affected = await entityManager.nativeUpdate(entity, where, {
      stock: raw('stock + ?', [adjustDto.delta])
    });

    if (affected === 0) {
      // Zero rows changed: either the variant has no inventory row, or the
      // guard blocked an oversell. Disambiguate so the caller gets a useful
      // error rather than a bare "not found".
      const existing = await entityManager.findOne(entity, {
        variantId: adjustDto.variantId
      });
      if (!existing) {
        throw new Error(
          `No inventory record for variant ${adjustDto.variantId}`
        );
      }
      throw new Error(
        `Insufficient stock for variant ${adjustDto.variantId}: have ${existing.stock}, requested change ${adjustDto.delta}`
      );
    }

    const inventory = await entityManager.findOneOrFail(entity, {
      variantId: adjustDto.variantId
    });
    return this.mappers.InventoryMapper.toDto(
      inventory as InferEntity<MapperEntities['InventoryMapper']>
    );
  }

  async checkStock(
    stockCheckDto: StockCheckDto,
    em?: EntityManager
  ): Promise<StockCheckResultDto> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Checking stock', stockCheckDto);
    }
    const inventory = await (em ?? this.em).findOneOrFail(
      this.mappers.InventoryMapper.entity as typeof Inventory,
      { variantId: stockCheckDto.variantId }
    );
    return {
      variantId: stockCheckDto.variantId,
      available: inventory.stock >= stockCheckDto.requested,
      stock: inventory.stock
    };
  }
}

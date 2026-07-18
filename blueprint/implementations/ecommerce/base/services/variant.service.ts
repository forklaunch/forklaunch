import { IdDto, IdsDto } from '@forklaunch/common';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { VariantService } from '@forklaunch/interfaces-ecommerce/interfaces';
import {
  CreateVariantDto,
  UpdateVariantDto
} from '@forklaunch/interfaces-ecommerce/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseVariantDtos } from '../domain/types/baseEcommerceDto.types';
import { BaseVariantEntities } from '../domain/types/baseEcommerceEntity.types';
import { VariantMappers } from '../domain/types/variant.mapper.types';
import { Variant } from '../persistence/entities';

export class BaseVariantService<
  SchemaValidator extends AnySchemaValidator,
  MapperEntities extends BaseVariantEntities,
  MapperDomains extends BaseVariantDtos = BaseVariantDtos
> implements VariantService
{
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: VariantMappers<MapperEntities, MapperDomains>;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: VariantMappers<MapperEntities, MapperDomains>,
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

  async listVariants(
    idsDto?: IdsDto,
    em?: EntityManager
  ): Promise<MapperDomains['VariantMapper'][]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Listing variants', idsDto);
    }
    return Promise.all(
      (
        await (em ?? this.em).findAll(
          this.mappers.VariantMapper.entity as typeof Variant,
          {
            where: idsDto?.ids?.length ? { id: { $in: idsDto.ids } } : undefined
          }
        )
      ).map((variant) =>
        this.mappers.VariantMapper.toDto(
          variant as InferEntity<MapperEntities['VariantMapper']>
        )
      )
    );
  }

  async listVariantsByProduct(
    productIdDto: { productId: string },
    em?: EntityManager
  ): Promise<MapperDomains['VariantMapper'][]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Listing variants by product', productIdDto);
    }
    return Promise.all(
      (
        await (em ?? this.em).findAll(
          this.mappers.VariantMapper.entity as typeof Variant,
          { where: { productId: productIdDto.productId } }
        )
      ).map((variant) =>
        this.mappers.VariantMapper.toDto(
          variant as InferEntity<MapperEntities['VariantMapper']>
        )
      )
    );
  }

  async createVariant(
    variantDto: CreateVariantDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['VariantMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Creating variant', variantDto);
    }
    const variant = await this.mappers.CreateVariantMapper.toEntity(
      variantDto,
      em ?? this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(variant);
    });
    return this.mappers.VariantMapper.toDto(variant);
  }

  async getVariant(
    idDto: IdDto,
    em?: EntityManager
  ): Promise<MapperDomains['VariantMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting variant', idDto);
    }
    const variant = await (em ?? this.em).findOneOrFail(
      this.mappers.VariantMapper.entity as typeof Variant,
      idDto
    );
    return this.mappers.VariantMapper.toDto(
      variant as InferEntity<MapperEntities['VariantMapper']>
    );
  }

  async getVariantByExternalId(
    externalIdDto: { externalId: string },
    em?: EntityManager
  ): Promise<MapperDomains['VariantMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Getting variant by externalId',
        externalIdDto
      );
    }
    const variant = await (em ?? this.em).findOneOrFail(
      this.mappers.VariantMapper.entity as typeof Variant,
      { externalId: externalIdDto.externalId }
    );
    return this.mappers.VariantMapper.toDto(
      variant as InferEntity<MapperEntities['VariantMapper']>
    );
  }

  async updateVariant(
    variantDto: UpdateVariantDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['VariantMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Updating variant', variantDto);
    }
    const variant = await this.mappers.UpdateVariantMapper.toEntity(
      variantDto,
      em ?? this.em,
      ...args
    );
    const updatedVariant = await (em ?? this.em).transactional(async (innerEm) => {
      return await innerEm.upsert(variant);
    });
    return this.mappers.VariantMapper.toDto(updatedVariant);
  }

  async deleteVariant(idDto: { id: string }, em?: EntityManager): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Deleting variant', idDto);
    }
    await (em ?? this.em).nativeDelete(
      this.mappers.VariantMapper.entity as typeof Variant,
      idDto
    );
  }
}

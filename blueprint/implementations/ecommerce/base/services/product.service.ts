import { IdDto } from '@forklaunch/common';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { ProductService } from '@forklaunch/interfaces-ecommerce/interfaces';
import {
  CreateProductDto,
  ProductSearchDto,
  UpdateProductDto
} from '@forklaunch/interfaces-ecommerce/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseProductDtos } from '../domain/types/baseEcommerceDto.types';
import { BaseProductEntities } from '../domain/types/baseEcommerceEntity.types';
import { ProductMappers } from '../domain/types/product.mapper.types';
import { Product } from '../persistence/entities';

export class BaseProductService<
  SchemaValidator extends AnySchemaValidator,
  MapperEntities extends BaseProductEntities,
  MapperDomains extends BaseProductDtos = BaseProductDtos
> implements ProductService
{
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: ProductMappers<MapperEntities, MapperDomains>;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: ProductMappers<MapperEntities, MapperDomains>,
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

  /**
   * Product-level half of catalog search/filter (ECOM-03): `ids` and
   * `title`. Price/stock/option-value filters are variant-level and are
   * resolved one layer up, at the deployable app's controller — this
   * service only knows about Product, not Variant/Inventory — which then
   * narrows to a candidate `ids` set before calling this.
   */
  async listProducts(
    searchDto?: ProductSearchDto,
    em?: EntityManager
  ): Promise<MapperDomains['ProductMapper'][]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Listing products', searchDto);
    }

    const where: Record<string, unknown> = {};
    if (searchDto?.ids?.length) {
      where.id = { $in: searchDto.ids };
    }
    if (searchDto?.title) {
      where.title = { $ilike: `%${searchDto.title}%` };
    }

    return Promise.all(
      (
        await (em ?? this.em).findAll(
          this.mappers.ProductMapper.entity as typeof Product,
          { where: Object.keys(where).length ? where : undefined }
        )
      ).map((product) =>
        this.mappers.ProductMapper.toDto(
          product as InferEntity<MapperEntities['ProductMapper']>
        )
      )
    );
  }

  async createProduct(
    productDto: CreateProductDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['ProductMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Creating product', productDto);
    }
    const product = await this.mappers.CreateProductMapper.toEntity(
      productDto,
      em ?? this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(product);
    });
    return this.mappers.ProductMapper.toDto(product);
  }

  async getProduct(
    idDto: IdDto,
    em?: EntityManager
  ): Promise<MapperDomains['ProductMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting product', idDto);
    }
    const product = await (em ?? this.em).findOneOrFail(
      this.mappers.ProductMapper.entity as typeof Product,
      idDto
    );
    return this.mappers.ProductMapper.toDto(
      product as InferEntity<MapperEntities['ProductMapper']>
    );
  }

  async getProductByHandle(
    handleDto: { handle: string },
    em?: EntityManager
  ): Promise<MapperDomains['ProductMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting product by handle', handleDto);
    }
    const product = await (em ?? this.em).findOneOrFail(
      this.mappers.ProductMapper.entity as typeof Product,
      { handle: handleDto.handle }
    );
    return this.mappers.ProductMapper.toDto(
      product as InferEntity<MapperEntities['ProductMapper']>
    );
  }

  async getProductByExternalId(
    externalIdDto: { externalId: string },
    em?: EntityManager
  ): Promise<MapperDomains['ProductMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Getting product by externalId',
        externalIdDto
      );
    }
    const product = await (em ?? this.em).findOneOrFail(
      this.mappers.ProductMapper.entity as typeof Product,
      { externalId: externalIdDto.externalId }
    );
    return this.mappers.ProductMapper.toDto(
      product as InferEntity<MapperEntities['ProductMapper']>
    );
  }

  async updateProduct(
    productDto: UpdateProductDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['ProductMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Updating product', productDto);
    }
    const product = await this.mappers.UpdateProductMapper.toEntity(
      productDto,
      em ?? this.em,
      ...args
    );
    const updatedProduct = await (em ?? this.em).upsert(product);
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(product);
    });
    return this.mappers.ProductMapper.toDto(updatedProduct);
  }

  async deleteProduct(idDto: { id: string }, em?: EntityManager): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Deleting product', idDto);
    }
    await (em ?? this.em).nativeDelete(
      this.mappers.ProductMapper.entity as typeof Product,
      idDto
    );
  }
}

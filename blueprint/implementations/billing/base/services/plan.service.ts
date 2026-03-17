import { IdDto, IdsDto } from '@forklaunch/common';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { PlanService } from '@forklaunch/interfaces-billing/interfaces';
import {
  CreatePlanDto,
  UpdatePlanDto
} from '@forklaunch/interfaces-billing/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import { BasePlanDtos } from '../domain/types/baseBillingDto.types';
import { BasePlanEntities } from '../domain/types/baseBillingEntity.types';
import { PlanMappers } from '../domain/types/plan.mapper.types';

export class BasePlanService<
  SchemaValidator extends AnySchemaValidator,
  PlanCadenceEnum,
  CurrencyEnum,
  BillingProviderEnum,
  MapperEntities extends BasePlanEntities<
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum
  >,
  MapperDomains extends BasePlanDtos<
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum
  > = BasePlanDtos<PlanCadenceEnum, CurrencyEnum, BillingProviderEnum>
> implements PlanService<PlanCadenceEnum, CurrencyEnum, BillingProviderEnum>
{
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: PlanMappers<
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum,
    MapperEntities,
    MapperDomains
  >;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: PlanMappers<
      PlanCadenceEnum,
      CurrencyEnum,
      BillingProviderEnum,
      MapperEntities,
      MapperDomains
    >,
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

  async listPlans(
    idsDto?: IdsDto,
    em?: EntityManager
  ): Promise<MapperDomains['PlanMapper'][]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Listing plans', idsDto);
    }

    return Promise.all(
      (
        await (em ?? this.em).findAll(this.mappers.PlanMapper.entity, {
          where: idsDto?.ids?.length ? { id: { $in: idsDto.ids } } : undefined
        })
      ).map((plan) => this.mappers.PlanMapper.toDto(plan))
    );
  }

  async createPlan(
    planDto: CreatePlanDto<PlanCadenceEnum, CurrencyEnum, BillingProviderEnum>,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['PlanMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Creating plan', planDto);
    }
    const plan = await this.mappers.CreatePlanMapper.toEntity(
      planDto,
      em ?? this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(plan);
    });
    return this.mappers.PlanMapper.toDto(plan);
  }

  async getPlan(
    idDto: IdDto,
    em?: EntityManager
  ): Promise<MapperDomains['PlanMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting plan', idDto);
    }
    const plan = await (em ?? this.em).findOneOrFail(
      this.mappers.PlanMapper.entity,
      idDto
    );
    return this.mappers.PlanMapper.toDto(plan);
  }

  async updatePlan(
    planDto: UpdatePlanDto<PlanCadenceEnum, CurrencyEnum, BillingProviderEnum>,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['PlanMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Updating plan', planDto);
    }
    const plan = await this.mappers.UpdatePlanMapper.toEntity(
      planDto,
      em ?? this.em,
      ...args
    );
    const updatedPlan = await (em ?? this.em).upsert(plan);
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(plan);
    });
    const updatedPlanDto = await this.mappers.PlanMapper.toDto(updatedPlan);
    return updatedPlanDto;
  }

  async deletePlan(idDto: { id: string }, em?: EntityManager): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Deleting plan', idDto);
    }
    await (em ?? this.em).nativeDelete(this.mappers.PlanMapper.entity, idDto);
  }
}

import { IdDto, IdsDto } from '@forklaunch/common';
import {
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { BasePlanService } from '@forklaunch/implementation-billing-base/services';
import { PlanMappers } from '@forklaunch/implementation-billing-base/types';
import { PlanService } from '@forklaunch/interfaces-billing/interfaces';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { BillingProviderEnum } from '../domain/enum/billingProvider.enum';
import { CurrencyEnum } from '../domain/enum/currency.enum';
import { PlanCadenceEnum } from '../domain/enum/planCadence.enum';
import { StripePlanMappers } from '../domain/types/plan.mapper.types';
import {
  StripeCreatePlanDto,
  StripePlanDto,
  StripePlanDtos,
  StripeUpdatePlanDto
} from '../domain/types/stripe.dto.types';
import { StripePlanEntities } from '../domain/types/stripe.entity.types';

export class StripePlanService<
  SchemaValidator extends AnySchemaValidator,
  Entities extends StripePlanEntities,
  Dto extends StripePlanDtos = StripePlanDtos
> implements PlanService<
  typeof PlanCadenceEnum,
  typeof CurrencyEnum,
  typeof BillingProviderEnum,
  {
    CreatePlanDto: StripeCreatePlanDto;
    UpdatePlanDto: StripeUpdatePlanDto;
    PlanDto: StripePlanDto;
    IdDto: IdDto;
    IdsDto: IdsDto;
  }
> {
  basePlanService: BasePlanService<
    SchemaValidator,
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum,
    Entities,
    Dto
  >;
  protected readonly stripeClient: Stripe;
  protected readonly em: EntityManager;
  protected readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected readonly schemaValidator: SchemaValidator;
  protected readonly mappers: StripePlanMappers<Entities, Dto>;

  constructor(
    stripeClient: Stripe,
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: StripePlanMappers<Entities, Dto>,
    readonly options?: {
      telemetry?: TelemetryOptions;
      databaseTableName?: string;
    }
  ) {
    this.stripeClient = stripeClient;
    this.em = em;
    this.openTelemetryCollector = openTelemetryCollector;
    this.schemaValidator = schemaValidator;
    this.mappers = mappers;
    this.basePlanService = new BasePlanService(
      em,
      openTelemetryCollector,
      schemaValidator,
      mappers as PlanMappers<
        PlanCadenceEnum,
        CurrencyEnum,
        BillingProviderEnum,
        Entities,
        Dto
      >,
      options
    );
  }

  async createPlan(
    planDto: StripeCreatePlanDto,
    em?: EntityManager
  ): Promise<Dto['PlanMapper']> {
    let stripePlan = await this.stripeClient.plans.create({
      ...planDto.stripeFields,
      amount: planDto.price,
      interval: planDto.cadence,
      product: planDto.name,
      currency: planDto.currency as string
    });

    // retrieve the plan with the product expanded
    stripePlan = await this.stripeClient.plans.retrieve(stripePlan.id, {
      expand: ['product']
    });

    const plan = await this.basePlanService.createPlan(
      {
        ...planDto,
        externalId: stripePlan.id,
        billingProvider: 'stripe'
      },
      em ?? this.em,
      stripePlan.product as Stripe.Product
    );

    return plan;
  }

  async getPlan(idDto: IdDto, em?: EntityManager): Promise<Dto['PlanMapper']> {
    const planEntity = await this.basePlanService.getPlan(idDto, em);
    if (!planEntity.externalId) {
      throw new Error('Plan not found');
    }
    const plan = await this.stripeClient.plans.retrieve(planEntity.externalId, {
      expand: ['product']
    });
    planEntity.stripeFields = plan.product as Stripe.Product;
    return planEntity;
  }

  async updatePlan(
    planDto: StripeUpdatePlanDto,
    em?: EntityManager
  ): Promise<Dto['PlanMapper']> {
    const planEntity = await this.basePlanService.getPlan(
      {
        id: planDto.id
      },
      em
    );

    const existingPlan = await this.stripeClient.plans.retrieve(
      planEntity.externalId
    );

    const existingProduct = existingPlan.product;
    if (!existingProduct) {
      throw new Error('Plan product not found');
    }

    const productId =
      typeof existingProduct === 'string'
        ? existingProduct
        : existingProduct.id;

    await this.stripeClient.plans.del(planEntity.externalId);
    let updatedPlan = await this.stripeClient.plans.create({
      ...planDto.stripeFields,
      interval:
        planDto.cadence ??
        // a fetched plan's interval was valid at creation, so it round-trips
        (existingPlan.interval as Stripe.PlanCreateParams.Interval),
      currency: planDto.currency ?? existingPlan.currency,
      amount: planDto.price ?? existingPlan.amount ?? undefined,
      product: productId
    });

    updatedPlan = await this.stripeClient.plans.retrieve(updatedPlan.id, {
      expand: ['product']
    });

    const updatedPlanEntity = await this.basePlanService.updatePlan(
      {
        ...planDto,
        externalId: updatedPlan.id,
        name: planDto.name,
        billingProvider: 'stripe'
      },
      em,
      updatedPlan.product as Stripe.Product
    );

    updatedPlanEntity.stripeFields = updatedPlan.product as Stripe.Product;

    return updatedPlanEntity;
  }

  async deletePlan(idDto: IdDto, em?: EntityManager): Promise<void> {
    const plan = await this.basePlanService.getPlan(idDto, em);
    if (!plan.externalId) {
      throw new Error('Plan not found');
    }
    await this.stripeClient.plans.del(plan.externalId);
    await this.basePlanService.deletePlan(idDto, em);
  }

  async listPlans(
    idsDto?: IdsDto,
    em?: EntityManager
  ): Promise<Dto['PlanMapper'][]> {
    const plans = await this.basePlanService.listPlans(idsDto, em);

    if (!plans || plans.length === 0) {
      return [];
    }

    const stripePlans = await Promise.all(
      plans.map(async (plan) => {
        try {
          const fetchedPlan = await this.stripeClient.plans.retrieve(
            plan.externalId,
            {
              expand: ['product']
            }
          );
          return fetchedPlan.product as Stripe.Product;
        } catch {
          return null;
        }
      })
    );

    return plans
      .map((plan, index) => ({
        ...plan,
        stripeFields: stripePlans[index]
      }))
      .filter((plan) => plan.stripeFields !== null);
  }
}

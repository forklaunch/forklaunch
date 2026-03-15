import { IdDto, IdsDto } from '@forklaunch/common';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { SubscriptionService } from '@forklaunch/interfaces-billing/interfaces';
import {
  CreateSubscriptionDto,
  UpdateSubscriptionDto
} from '@forklaunch/interfaces-billing/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import { BaseSubscriptionDtos } from '../domain/types/baseBillingDto.types';
import { BaseSubscriptionEntities } from '../domain/types/baseBillingEntity.types';
import { SubscriptionMappers } from '../domain/types/subscription.mapper.types';

export class BaseSubscriptionService<
  SchemaValidator extends AnySchemaValidator,
  PartyType,
  BillingProviderType,
  Entities extends BaseSubscriptionEntities<PartyType, BillingProviderType>,
  Dto extends BaseSubscriptionDtos<
    PartyType,
    BillingProviderType
  > = BaseSubscriptionDtos<PartyType, BillingProviderType>
> implements SubscriptionService<PartyType, BillingProviderType>
{
  protected evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  protected em: EntityManager;
  protected readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected readonly schemaValidator: SchemaValidator;
  protected readonly mappers: SubscriptionMappers<
    PartyType,
    BillingProviderType,
    Entities,
    Dto
  >;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: SubscriptionMappers<PartyType, BillingProviderType, Entities, Dto>,
    readonly options?: {
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

  async createSubscription(
    subscriptionDto: CreateSubscriptionDto<PartyType, BillingProviderType>,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<Dto['SubscriptionMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Creating subscription',
        subscriptionDto
      );
    }
    const subscription = await this.mappers.CreateSubscriptionMapper.toEntity(
      subscriptionDto,
      em ?? this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(subscription);
    });
    const createdSubscriptionDto =
      await this.mappers.SubscriptionMapper.toDto(subscription);
    return createdSubscriptionDto;
  }

  async getSubscription(
    idDto: IdDto,
    em?: EntityManager
  ): Promise<Dto['SubscriptionMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting subscription', idDto);
    }
    const subscription = await (em ?? this.em).findOneOrFail(
      this.mappers.SubscriptionMapper.entity,
      idDto
    );
    return this.mappers.SubscriptionMapper.toDto(
      subscription as Entities['SubscriptionMapper']
    );
  }

  async getUserSubscription(
    { id }: IdDto,
    em?: EntityManager
  ): Promise<Dto['SubscriptionMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting user subscription', id);
    }
    const subscription = await (em ?? this.em).findOneOrFail(
      this.mappers.SubscriptionMapper.entity,
      {
        partyId: id,
        partyType: 'USER',
        active: true
      }
    );

    return this.mappers.SubscriptionMapper.toDto(
      subscription as Entities['SubscriptionMapper']
    );
  }

  async getOrganizationSubscription(
    { id }: IdDto,
    em?: EntityManager
  ): Promise<Dto['SubscriptionMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting organization subscription', id);
    }
    const subscription = await (em ?? this.em).findOneOrFail(
      this.mappers.SubscriptionMapper.entity,
      {
        partyId: id,
        partyType: 'ORGANIZATION',
        active: true
      }
    );
    return this.mappers.SubscriptionMapper.toDto(
      subscription as Entities['SubscriptionMapper']
    );
  }

  async updateSubscription(
    subscriptionDto: UpdateSubscriptionDto<PartyType, BillingProviderType>,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<Dto['SubscriptionMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Updating subscription',
        subscriptionDto
      );
    }
    const subscription = await this.mappers.UpdateSubscriptionMapper.toEntity(
      subscriptionDto,
      em ?? this.em,
      ...args
    );
    const updatedSubscription = await (em ?? this.em).upsert(subscription);
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(updatedSubscription);
    });
    const updatedSubscriptionDto =
      await this.mappers.SubscriptionMapper.toDto(updatedSubscription);

    return updatedSubscriptionDto;
  }

  async deleteSubscription(
    idDto: { id: string },
    em?: EntityManager
  ): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Deleting subscription', idDto);
    }
    const subscription = await (em ?? this.em).findOne(
      this.mappers.SubscriptionMapper.entity,
      idDto
    );
    if (!subscription) {
      throw new Error('Subscription not found');
    }
    await (em ?? this.em).remove(subscription).flush();
  }

  async listSubscriptions(
    idsDto?: IdsDto,
    em?: EntityManager
  ): Promise<Dto['SubscriptionMapper'][]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Listing subscriptions', idsDto);
    }
    return Promise.all(
      (
        await (em ?? this.em).findAll(this.mappers.SubscriptionMapper.entity, {
          where: idsDto?.ids?.length ? { id: { $in: idsDto.ids } } : undefined
        })
      ).map((subscription) =>
        this.mappers.SubscriptionMapper.toDto(
          subscription as Entities['SubscriptionMapper']
        )
      )
    );
  }

  async cancelSubscription(idDto: IdDto, em?: EntityManager): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Canceling subscription', idDto);
    }
    const subscription = await (em ?? this.em).findOne(
      this.mappers.SubscriptionMapper.entity,
      idDto
    );
    if (!subscription) {
      throw new Error('Subscription not found');
    }
    (subscription as Entities['SubscriptionMapper']).active = false;
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(subscription);
    });
  }

  async resumeSubscription(idDto: IdDto, em?: EntityManager): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Resuming subscription', idDto);
    }
    const subscription = await (em ?? this.em).findOne(
      this.mappers.SubscriptionMapper.entity,
      idDto
    );
    if (!subscription) {
      throw new Error('Subscription not found');
    }
    (subscription as Entities['SubscriptionMapper']).active = true;
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(subscription);
    });
  }
}

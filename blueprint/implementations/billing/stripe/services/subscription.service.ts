import { IdDto, IdsDto } from '@forklaunch/common';
import {
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { BaseSubscriptionService } from '@forklaunch/implementation-billing-base/services';
import { SubscriptionMappers } from '@forklaunch/implementation-billing-base/types';
import { SubscriptionService } from '@forklaunch/interfaces-billing/interfaces';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { BillingProviderEnum } from '../domain/enum/billingProvider.enum';
import {
  StripeCreateSubscriptionDto,
  StripeSubscriptionDto,
  StripeSubscriptionDtos,
  StripeUpdateSubscriptionDto
} from '../domain/types/stripe.dto.types';
import { StripeSubscriptionEntities } from '../domain/types/stripe.entity.types';
import { StripeSubscriptionMappers } from '../domain/types/subscription.mapper.types';

export class StripeSubscriptionService<
  SchemaValidator extends AnySchemaValidator,
  PartyType,
  Entities extends StripeSubscriptionEntities,
  Dto extends StripeSubscriptionDtos<PartyType> =
    StripeSubscriptionDtos<PartyType>
> implements SubscriptionService<
  PartyType,
  typeof BillingProviderEnum,
  {
    CreateSubscriptionDto: StripeCreateSubscriptionDto<PartyType>;
    UpdateSubscriptionDto: StripeUpdateSubscriptionDto<PartyType>;
    SubscriptionDto: StripeSubscriptionDto<PartyType>;
    IdDto: IdDto;
    IdsDto: IdsDto;
  }
> {
  baseSubscriptionService: BaseSubscriptionService<
    SchemaValidator,
    PartyType,
    BillingProviderEnum,
    Entities,
    Dto
  >;
  protected readonly stripeClient: Stripe;
  protected readonly em: EntityManager;
  protected readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected readonly schemaValidator: SchemaValidator;
  protected readonly mappers: StripeSubscriptionMappers<
    PartyType,
    Entities,
    Dto
  >;

  constructor(
    stripeClient: Stripe,
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: StripeSubscriptionMappers<PartyType, Entities, Dto>,
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
    this.baseSubscriptionService = new BaseSubscriptionService(
      em,
      openTelemetryCollector,
      schemaValidator,
      mappers as SubscriptionMappers<
        PartyType,
        BillingProviderEnum,
        Entities,
        Dto
      >,
      options
    );
  }

  async createSubscription(
    subscriptionDto: StripeCreateSubscriptionDto<PartyType>,
    em?: EntityManager
  ): Promise<Dto['SubscriptionMapper']> {
    const subscription = await this.stripeClient.subscriptions.create({
      ...subscriptionDto.stripeFields,
      customer: subscriptionDto.partyId,
      items: [
        {
          plan: subscriptionDto.productId
        }
      ]
    });

    return await this.baseSubscriptionService.createSubscription(
      {
        ...subscriptionDto,
        externalId: subscription.id,
        billingProvider: 'stripe'
      },
      em ?? this.em,
      subscription
    );
  }

  async getSubscription(
    idDto: IdDto,
    em?: EntityManager
  ): Promise<Dto['SubscriptionMapper']> {
    const subscriptionEntity =
      await this.baseSubscriptionService.getSubscription(idDto, em);
    if (!subscriptionEntity.externalId) {
      throw new Error('Subscription not found');
    }
    const stripeSubscription = await this.stripeClient.subscriptions.retrieve(
      subscriptionEntity.externalId
    );
    subscriptionEntity.stripeFields = stripeSubscription;
    return subscriptionEntity;
  }

  async getUserSubscription(
    idDto: IdDto,
    em?: EntityManager
  ): Promise<Dto['SubscriptionMapper']> {
    const subscriptionEntity =
      await this.baseSubscriptionService.getUserSubscription(idDto, em);
    if (!subscriptionEntity.externalId) {
      throw new Error('Subscription not found');
    }
    const stripeSubscription = await this.stripeClient.subscriptions.retrieve(
      subscriptionEntity.externalId
    );
    subscriptionEntity.stripeFields = stripeSubscription;
    return subscriptionEntity;
  }

  async getOrganizationSubscription(
    idDto: IdDto,
    em?: EntityManager
  ): Promise<Dto['SubscriptionMapper']> {
    const subscriptionEntity =
      await this.baseSubscriptionService.getOrganizationSubscription(idDto, em);
    if (!subscriptionEntity.externalId) {
      throw new Error('Subscription not found');
    }
    const stripeSubscription = await this.stripeClient.subscriptions.retrieve(
      subscriptionEntity.externalId
    );
    subscriptionEntity.stripeFields = stripeSubscription;
    return subscriptionEntity;
  }

  async updateSubscription(
    subscriptionDto: StripeUpdateSubscriptionDto<PartyType> & IdDto,
    em?: EntityManager
  ): Promise<Dto['SubscriptionMapper']> {
    const subscriptionEntity =
      await this.baseSubscriptionService.getSubscription(
        {
          id: subscriptionDto.id
        },
        em
      );
    if (!subscriptionEntity.externalId) {
      throw new Error('Subscription not found');
    }

    const existingStripeSubscription =
      await this.stripeClient.subscriptions.retrieve(
        subscriptionEntity.externalId
      );

    const updatedSubscription = await this.stripeClient.subscriptions.update(
      subscriptionEntity.externalId,
      {
        ...subscriptionDto.stripeFields,
        items: existingStripeSubscription.items.data.map((item) => ({
          id: item.id,
          plan: subscriptionDto.productId || item.plan?.id
        }))
      }
    );

    const updatedSubscriptionEntity =
      await this.baseSubscriptionService.updateSubscription(
        {
          ...subscriptionDto,
          externalId: updatedSubscription.id,
          billingProvider: 'stripe'
        },
        em ?? this.em,
        updatedSubscription
      );

    updatedSubscriptionEntity.stripeFields = updatedSubscription;
    return updatedSubscriptionEntity;
  }

  async deleteSubscription(idDto: IdDto, em?: EntityManager): Promise<void> {
    const subscription = await this.baseSubscriptionService.getSubscription(
      idDto,
      em
    );
    if (!subscription.externalId) {
      throw new Error('Subscription not found');
    }
    await this.stripeClient.subscriptions.cancel(subscription.externalId);
    await this.baseSubscriptionService.deleteSubscription(idDto, em);
  }

  async listSubscriptions(
    idsDto?: IdsDto,
    em?: EntityManager
  ): Promise<Dto['SubscriptionMapper'][]> {
    const subscriptions = await this.baseSubscriptionService.listSubscriptions(
      idsDto,
      em
    );

    if (!subscriptions || subscriptions.length === 0) {
      return [];
    }

    const stripeSubscriptions = await Promise.all(
      subscriptions.map(async (subscription) => {
        try {
          return await this.stripeClient.subscriptions.retrieve(
            subscription.externalId
          );
        } catch {
          return null;
        }
      })
    );

    return subscriptions
      .map((subscription, index) => ({
        ...subscription,
        stripeFields: stripeSubscriptions[index]
      }))
      .filter((subscription) => subscription.stripeFields !== null);
  }

  async cancelSubscription(idDto: IdDto, em?: EntityManager): Promise<void> {
    const subscription = await this.baseSubscriptionService.getSubscription(
      idDto,
      em
    );
    if (!subscription.externalId) {
      throw new Error('Subscription not found');
    }
    await this.stripeClient.subscriptions.cancel(subscription.externalId);
    await this.baseSubscriptionService.cancelSubscription(idDto, em);
  }

  async resumeSubscription(idDto: IdDto, em?: EntityManager): Promise<void> {
    const subscription = await this.baseSubscriptionService.getSubscription(
      idDto,
      em
    );
    if (!subscription.externalId) {
      throw new Error('Subscription not found');
    }
    await this.stripeClient.subscriptions.resume(subscription.externalId);
    await this.baseSubscriptionService.resumeSubscription(idDto, em);
  }
}

import { IdDto, IdsDto } from '@forklaunch/common';
import { TtlCache } from '@forklaunch/core/cache';
import {
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { BasePaymentLinkService } from '@forklaunch/implementation-billing-base/services';
import { PaymentLinkMappers } from '@forklaunch/implementation-billing-base/types';
import { PaymentLinkService } from '@forklaunch/interfaces-billing/interfaces';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { CurrencyEnum, PaymentMethodEnum } from '../domain/enum';
import { StripePaymentLinkMappers } from '../domain/types/paymentLink.mapper.types';
import {
  StripeCreatePaymentLinkDto,
  StripePaymentLinkDto,
  StripePaymentLinkDtos,
  StripeUpdatePaymentLinkDto
} from '../domain/types/stripe.dto.types';
import { StripePaymentLinkEntities } from '../domain/types/stripe.entity.types';

export class StripePaymentLinkService<
  SchemaValidator extends AnySchemaValidator,
  StatusEnum,
  Entities extends StripePaymentLinkEntities<StatusEnum>,
  Dto extends
    StripePaymentLinkDtos<StatusEnum> = StripePaymentLinkDtos<StatusEnum>
> implements
    PaymentLinkService<
      PaymentMethodEnum,
      typeof CurrencyEnum,
      StatusEnum,
      {
        CreatePaymentLinkDto: StripeCreatePaymentLinkDto<StatusEnum>;
        UpdatePaymentLinkDto: StripeUpdatePaymentLinkDto<StatusEnum>;
        PaymentLinkDto: StripePaymentLinkDto<StatusEnum>;
        IdDto: IdDto;
        IdsDto: IdsDto;
      }
    >
{
  basePaymentLinkService: BasePaymentLinkService<
    SchemaValidator,
    PaymentMethodEnum,
    CurrencyEnum,
    StatusEnum,
    Entities,
    Dto
  >;
  protected readonly stripeClient: Stripe;
  protected readonly em: EntityManager;
  protected readonly cache: TtlCache;
  protected readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected readonly schemaValidator: SchemaValidator;
  protected readonly mappers: StripePaymentLinkMappers<
    StatusEnum,
    Entities,
    Dto
  >;

  constructor(
    stripeClient: Stripe,
    em: EntityManager,
    cache: TtlCache,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: StripePaymentLinkMappers<StatusEnum, Entities, Dto>,
    readonly options?: {
      enableDatabaseBackup?: boolean;
      telemetry?: TelemetryOptions;
    }
  ) {
    this.stripeClient = stripeClient;
    this.em = em;
    this.cache = cache;
    this.openTelemetryCollector = openTelemetryCollector;
    this.schemaValidator = schemaValidator;
    this.mappers = mappers;
    this.basePaymentLinkService = new BasePaymentLinkService(
      em,
      cache,
      openTelemetryCollector,
      schemaValidator,
      mappers as PaymentLinkMappers<
        PaymentMethodEnum,
        CurrencyEnum,
        StatusEnum,
        Entities,
        Dto
      >,
      options
    );
  }

  async createPaymentLink(
    paymentLinkDto: StripeCreatePaymentLinkDto<StatusEnum>,
    ...args: unknown[]
  ): Promise<Dto['PaymentLinkMapper']> {
    const session = await this.stripeClient.paymentLinks.create({
      ...paymentLinkDto.stripeFields,
      payment_method_types: paymentLinkDto.paymentMethods,
      currency: paymentLinkDto.currency as string
    });

    const paymentLink = await this.basePaymentLinkService.createPaymentLink(
      {
        ...paymentLinkDto,
        id: session.id,
        amount:
          paymentLinkDto.amount ??
          session.line_items?.data.reduce<number>(
            (total, item) => total + item.amount_total,
            0
          )
      },
      this.em,
      session,
      ...args
    );

    paymentLink.stripeFields = session;

    return paymentLink;
  }

  async updatePaymentLink(
    paymentLinkDto: StripeUpdatePaymentLinkDto<StatusEnum>,
    ...args: unknown[]
  ): Promise<Dto['PaymentLinkMapper']> {
    const session = await this.stripeClient.paymentLinks.update(
      paymentLinkDto.id,
      {
        ...paymentLinkDto.stripeFields,
        payment_method_types: paymentLinkDto.paymentMethods
      }
    );

    const paymentLink = await this.basePaymentLinkService.updatePaymentLink(
      await this.mappers.PaymentLinkMapper.toDto(
        await this.mappers.UpdatePaymentLinkMapper.toEntity(
          {
            ...paymentLinkDto,
            id: session.id,
            amount:
              session.line_items?.data.reduce<number>(
                (total, item) => total + item.amount_total,
                0
              ) ?? 0
          },
          this.em,
          session
        )
      ),
      ...args
    );

    paymentLink.stripeFields = session;

    return paymentLink;
  }

  async getPaymentLink({ id }: IdDto): Promise<Dto['PaymentLinkMapper']> {
    const stripePaymentLink = await this.stripeClient.paymentLinks.retrieve(id);

    const databasePaymentLink =
      await this.basePaymentLinkService.getPaymentLink({ id });

    databasePaymentLink.stripeFields = stripePaymentLink;

    return databasePaymentLink;
  }

  async expirePaymentLink({ id }: IdDto): Promise<void> {
    await this.stripeClient.paymentLinks.update(id, {
      metadata: {
        status: 'EXPIRED'
      }
    });
    await this.basePaymentLinkService.expirePaymentLink({ id });
  }

  async handlePaymentSuccess({ id }: IdDto): Promise<void> {
    await this.stripeClient.paymentLinks.update(id, {
      metadata: {
        status: 'COMPLETED'
      }
    });
    await this.basePaymentLinkService.handlePaymentSuccess({ id });
  }

  async handlePaymentFailure({ id }: IdDto): Promise<void> {
    await this.stripeClient.paymentLinks.update(id, {
      metadata: {
        status: 'FAILED'
      }
    });
    await this.basePaymentLinkService.handlePaymentFailure({ id });
  }

  async listPaymentLinks(idsDto?: IdsDto): Promise<Dto['PaymentLinkMapper'][]> {
    this.openTelemetryCollector.log('info', idsDto ?? 'idsDto is undefined');
    const stripePaymentLinks = await this.stripeClient.paymentLinks.list({
      active: true
    });

    const databasePaymentLinks =
      await this.basePaymentLinkService.listPaymentLinks(idsDto);

    return databasePaymentLinks
      .map((paymentLink) => {
        const stripePaymentLink = stripePaymentLinks.data.find(
          (sp) => sp.id === paymentLink.id
        );
        if (!stripePaymentLink) {
          return null;
        }
        paymentLink.stripeFields = stripePaymentLink;
        return paymentLink;
      })
      .filter((paymentLink) => paymentLink !== null);
  }
}

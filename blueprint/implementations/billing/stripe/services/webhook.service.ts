import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { PartyEnum } from '../../../../billing-base/domain/enum/party.enum';
import { StripeWebhookEvent } from '../../../../billing-stripe/persistence/entities/stripeWebhookEvent.entity';
import { BillingProviderEnum } from '../domain/enum/billingProvider.enum';
import { CurrencyEnum } from '../domain/enum/currency.enum';
import { PaymentMethodEnum } from '../domain/enum/paymentMethod.enum';
import { PlanCadenceEnum } from '../domain/enum/planCadence.enum';
import {
  StripeBillingPortalEntities,
  StripeCheckoutSessionEntities,
  StripePaymentLinkEntities,
  StripePlanEntities,
  StripeSubscriptionEntities
} from '../domain/types/stripe.entity.types';
import { StripeBillingPortalService } from './billingPortal.service';
import { StripeCheckoutSessionService } from './checkoutSession.service';
import { StripePaymentLinkService } from './paymentLink.service';
import { StripePlanService } from './plan.service';
import { StripeSubscriptionService } from './subscription.service';

export class StripeWebhookService<
  SchemaValidator extends AnySchemaValidator,
  StatusEnum,
  PartyEnum,
  BillingPortalEntities extends
    StripeBillingPortalEntities = StripeBillingPortalEntities,
  CheckoutSessionEntities extends
    StripeCheckoutSessionEntities<StatusEnum> = StripeCheckoutSessionEntities<StatusEnum>,
  PaymentLinkEntities extends
    StripePaymentLinkEntities<StatusEnum> = StripePaymentLinkEntities<StatusEnum>,
  PlanEntities extends StripePlanEntities = StripePlanEntities,
  SubscriptionEntities extends
    StripeSubscriptionEntities<PartyEnum> = StripeSubscriptionEntities<PartyEnum>
> {
  protected readonly stripeClient: Stripe;
  protected readonly em: EntityManager;
  protected readonly schemaValidator: SchemaValidator;
  protected readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected readonly billingPortalService: StripeBillingPortalService<
    SchemaValidator,
    BillingPortalEntities
  >;
  protected readonly checkoutSessionService: StripeCheckoutSessionService<
    SchemaValidator,
    StatusEnum,
    CheckoutSessionEntities
  >;
  protected readonly paymentLinkService: StripePaymentLinkService<
    SchemaValidator,
    StatusEnum,
    PaymentLinkEntities
  >;
  protected readonly planService: StripePlanService<
    SchemaValidator,
    PlanEntities
  >;
  protected readonly subscriptionService: StripeSubscriptionService<
    SchemaValidator,
    PartyEnum,
    SubscriptionEntities
  >;

  constructor(
    stripeClient: Stripe,
    em: EntityManager,
    schemaValidator: SchemaValidator,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    billingPortalService: StripeBillingPortalService<
      SchemaValidator,
      BillingPortalEntities
    >,
    checkoutSessionService: StripeCheckoutSessionService<
      SchemaValidator,
      StatusEnum,
      CheckoutSessionEntities
    >,
    paymentLinkService: StripePaymentLinkService<
      SchemaValidator,
      StatusEnum,
      PaymentLinkEntities
    >,
    planService: StripePlanService<SchemaValidator, PlanEntities>,
    subscriptionService: StripeSubscriptionService<
      SchemaValidator,
      PartyEnum,
      SubscriptionEntities
    >
  ) {
    this.stripeClient = stripeClient;
    this.em = em;
    this.schemaValidator = schemaValidator;
    this.openTelemetryCollector = openTelemetryCollector;
    this.billingPortalService = billingPortalService;
    this.checkoutSessionService = checkoutSessionService;
    this.paymentLinkService = paymentLinkService;
    this.planService = planService;
    this.subscriptionService = subscriptionService;
  }

  /**
   * Extract features from Stripe product metadata.
   * Features can be stored as:
   * - metadata.features: comma-separated string (e.g., "feature1,feature2,feature3")
   * - metadata.features: JSON array string (e.g., '["feature1","feature2"]')
   */
  private extractFeaturesFromProduct(product: Stripe.Product): string[] {
    const featuresStr = product.metadata?.features;
    if (!featuresStr) {
      return [];
    }

    // Try parsing as JSON array first
    try {
      const parsed = JSON.parse(featuresStr);
      if (Array.isArray(parsed)) {
        return parsed.filter((f): f is string => typeof f === 'string');
      }
    } catch {
      // Not JSON, treat as comma-separated
    }

    return featuresStr
      .split(',')
      .map((f) => f.trim())
      .filter((f) => f.length > 0);
  }

  async handleWebhookEvent(event: Stripe.Event): Promise<void> {
    if (this.openTelemetryCollector) {
      this.openTelemetryCollector.info('Handling webhook event', event);
    }

    if (
      await this.em.findOne(StripeWebhookEvent, {
        idempotencyKey: event.request?.idempotency_key
      })
    ) {
      this.openTelemetryCollector.info(
        'Webhook event already processed',
        event
      );
      return;
    }

    const eventType = event.type;

    switch (eventType) {
      case 'billing_portal.session.created': {
        await this.billingPortalService.baseBillingPortalService.createBillingPortalSession(
          {
            id: event.data.object.id,
            customerId: event.data.object.customer,
            expiresAt: new Date(event.data.object.created + 5 * 60 * 1000),
            uri: event.data.object.url
          }
        );
        break;
      }

      case 'checkout.session.expired': {
        await this.checkoutSessionService.handleCheckoutFailure({
          id: event.data.object.id
        });
        break;
      }

      case 'checkout.session.completed': {
        await this.checkoutSessionService.handleCheckoutSuccess({
          id: event.data.object.id
        });
        break;
      }

      case 'payment_link.created':
        {
          await this.paymentLinkService.basePaymentLinkService.createPaymentLink(
            {
              id: event.data.object.id,
              amount:
                event.data.object.line_items?.data.reduce<number>(
                  (total, item) => total + item.amount_total,
                  0
                ) ?? 0,
              paymentMethods: event.data.object
                .payment_method_types as PaymentMethodEnum[],
              status: 'CREATED' as StatusEnum[keyof StatusEnum],
              currency: event.data.object.currency as CurrencyEnum
            }
          );
        }
        break;

      case 'payment_link.updated': {
        await this.paymentLinkService.basePaymentLinkService.updatePaymentLink({
          id: event.data.object.id,
          amount:
            event.data.object.line_items?.data.reduce<number>(
              (total, item) => total + item.amount_total,
              0
            ) ?? 0,
          paymentMethods: event.data.object
            .payment_method_types as PaymentMethodEnum[],
          status: 'UPDATED' as StatusEnum[keyof StatusEnum],
          currency: event.data.object.currency as CurrencyEnum
        });
        break;
      }

      case 'plan.created': {
        if (
          event.data.object.product != null &&
          event.data.object.amount != null
        ) {
          const productId =
            typeof event.data.object.product === 'string'
              ? event.data.object.product
              : event.data.object.product.id;
          const product = await this.stripeClient.products.retrieve(productId);
          const features = this.extractFeaturesFromProduct(product);

          await this.planService.basePlanService.createPlan({
            id: event.data.object.id,
            billingProvider: BillingProviderEnum.STRIPE,
            cadence: event.data.object.interval as PlanCadenceEnum,
            currency: event.data.object.currency as CurrencyEnum,
            active: product.active,
            name: product.name,
            price: event.data.object.amount,
            externalId: event.data.object.id,
            features
          });
        } else {
          throw new Error('Invalid plan');
        }
        break;
      }

      case 'plan.updated': {
        if (
          event.data.object.product != null &&
          event.data.object.amount != null
        ) {
          const productId =
            typeof event.data.object.product === 'string'
              ? event.data.object.product
              : event.data.object.product.id;
          const product = await this.stripeClient.products.retrieve(productId);
          const features = this.extractFeaturesFromProduct(product);

          await this.planService.basePlanService.updatePlan({
            id: event.data.object.id,
            billingProvider: BillingProviderEnum.STRIPE,
            cadence: event.data.object.interval as PlanCadenceEnum,
            currency: event.data.object.currency as CurrencyEnum,
            active: product.active,
            name: product.name,
            price: event.data.object.amount,
            externalId: event.data.object.id,
            features
          });
        } else {
          throw new Error('Invalid plan');
        }
        break;
      }

      case 'plan.deleted': {
        await this.planService.deletePlan({
          id: event.data.object.id
        });
        break;
      }

      case 'product.created':
      case 'product.updated': {
        // When a product is created/updated, sync features to all associated plans
        const product = event.data.object;
        const features = this.extractFeaturesFromProduct(product);

        // Update all legacy plans (iterates through all pages)
        await this.stripeClient.plans
          .list({ product: product.id })
          .autoPagingEach(async (plan) => {
            try {
              await this.planService.basePlanService.updatePlan({
                id: plan.id,
                features,
                active: product.active,
                name: product.name
              });
            } catch (error) {
              this.openTelemetryCollector.warn(
                `Failed to update plan ${plan.id} with product features`,
                error
              );
            }
          });

        // Update all price-based plans (iterates through all pages)
        await this.stripeClient.prices
          .list({ product: product.id })
          .autoPagingEach(async (price) => {
            try {
              await this.planService.basePlanService.updatePlan({
                id: price.id,
                features,
                active: price.active && product.active,
                name: product.name
              });
            } catch (error) {
              this.openTelemetryCollector.warn(
                `Failed to update price-based plan ${price.id} with product features`,
                error
              );
            }
          });
        break;
      }

      // Handle Stripe Prices API (newer alternative to Plans)
      case 'price.created':
      case 'price.updated': {
        const price = event.data.object;
        if (
          price.product != null &&
          price.unit_amount != null &&
          price.recurring
        ) {
          const productId =
            typeof price.product === 'string'
              ? price.product
              : price.product.id;
          const product = await this.stripeClient.products.retrieve(productId);
          const features = this.extractFeaturesFromProduct(product);

          const planData = {
            id: price.id,
            billingProvider: BillingProviderEnum.STRIPE,
            cadence: price.recurring.interval as PlanCadenceEnum,
            currency: price.currency as CurrencyEnum,
            active: price.active && product.active,
            name: product.name,
            price: price.unit_amount,
            externalId: price.id,
            features
          };

          if (event.type === 'price.created') {
            await this.planService.basePlanService.createPlan(planData);
          } else {
            await this.planService.basePlanService.updatePlan(planData);
          }
        }
        break;
      }

      case 'customer.subscription.created': {
        if (
          !event.data.object.items?.data ||
          event.data.object.items.data.length === 0 ||
          !event.data.object.items.data[0]?.plan?.id
        ) {
          throw new Error(
            `Invalid subscription: missing items or plan ID for subscription ${event.data.object.id}`
          );
        }
        await this.subscriptionService.baseSubscriptionService.createSubscription(
          {
            id: event.data.object.id,
            partyId:
              typeof event.data.object.customer === 'string'
                ? event.data.object.customer
                : event.data.object.customer.id,
            partyType: PartyEnum.USER as PartyEnum[keyof PartyEnum],
            description: event.data.object.description ?? undefined,
            active: true,
            productId: event.data.object.items.data[0].plan.id,
            externalId: event.data.object.id,
            billingProvider: BillingProviderEnum.STRIPE,
            startDate: new Date(event.data.object.created * 1000),
            endDate: event.data.object.cancel_at
              ? new Date(event.data.object.cancel_at * 1000)
              : undefined,
            status: event.data.object.status
          }
        );
        break;
      }

      case 'customer.subscription.updated': {
        if (
          !event.data.object.items?.data ||
          event.data.object.items.data.length === 0 ||
          !event.data.object.items.data[0]?.plan?.id
        ) {
          throw new Error(
            `Invalid subscription: missing items or plan ID for subscription ${event.data.object.id}`
          );
        }
        await this.subscriptionService.baseSubscriptionService.updateSubscription(
          {
            id: event.data.object.id,
            partyId:
              typeof event.data.object.customer === 'string'
                ? event.data.object.customer
                : event.data.object.customer.id,
            partyType: PartyEnum.USER as PartyEnum[keyof PartyEnum],
            description: event.data.object.description ?? undefined,
            active: true,
            externalId: event.data.object.id,
            billingProvider: BillingProviderEnum.STRIPE,
            startDate: new Date(event.data.object.created * 1000),
            endDate: event.data.object.cancel_at
              ? new Date(event.data.object.cancel_at * 1000)
              : undefined,
            productId: event.data.object.items.data[0].plan.id,
            status: event.data.object.status
          }
        );
        break;
      }

      case 'customer.subscription.deleted': {
        await this.subscriptionService.deleteSubscription({
          id: event.data.object.id
        });
        break;
      }

      case 'customer.subscription.paused': {
        await this.subscriptionService.cancelSubscription({
          id: event.data.object.id
        });
        break;
      }

      case 'customer.subscription.resumed': {
        await this.subscriptionService.resumeSubscription({
          id: event.data.object.id
        });
        break;
      }

      default:
        this.openTelemetryCollector.info(
          'Unprocessed stripe event type',
          eventType
        );
        break;
    }

    await this.em.insert(StripeWebhookEvent, {
      stripeId: event.id,
      idempotencyKey: event.request?.idempotency_key,
      eventType: event.type,
      eventData: event.data
    });
    await this.em.flush();
  }
}

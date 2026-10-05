import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager, EntityName } from '@mikro-orm/core';
import Stripe from 'stripe';
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

/**
 * Webhook idempotency records are looked up and written BY NAME so they
 * resolve against the consuming application's discovered metadata. Passing
 * this package's internal `StripeWebhookEvent` object to the EntityManager
 * queries an entity the app's ORM never discovered, which mikro-orm 7.1.x
 * rejects deep in EntityLoader (`meta.relations` is undefined for
 * undiscovered entities). Applications must discover an entity named
 * `StripeWebhookEvent` — either their own definition (the blueprint app
 * ships one) or this package's, importable from
 * `@forklaunch/implementation-billing-stripe/persistence`.
 */

/**
 * Structural constraint for injectable webhook event entities — the fields
 * the idempotency flow reads and writes. Applications inject their own
 * (typically richer, sqlBaseProperties-based) entity; the generic infers
 * from the injected schema exactly like the mapper entity generics do.
 */
export type StripeWebhookEventShape = {
  stripeId: string;
  idempotencyKey?: string | null;
  eventType: string;
  eventData: unknown;
};

/** Stripe statuses under which the subscription still grants access. */
const isActiveStatus = (status: Stripe.Subscription.Status): boolean =>
  status === 'active' || status === 'trialing';

export class StripeWebhookService<
  SchemaValidator extends AnySchemaValidator,
  StatusEnum,
  PartyEnum,
  BillingPortalEntities extends
    StripeBillingPortalEntities = StripeBillingPortalEntities,
  CheckoutSessionEntities extends
    StripeCheckoutSessionEntities = StripeCheckoutSessionEntities,
  PaymentLinkEntities extends
    StripePaymentLinkEntities = StripePaymentLinkEntities,
  PlanEntities extends StripePlanEntities = StripePlanEntities,
  SubscriptionEntities extends
    StripeSubscriptionEntities = StripeSubscriptionEntities,
  WebhookEventEntity extends StripeWebhookEventShape = StripeWebhookEventShape
> {
  protected readonly partyEnum: PartyEnum;
  protected readonly webhookEventEntity: { '~entity': WebhookEventEntity };
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
    >,
    partyEnum: PartyEnum,
    /**
     * The entity used for webhook idempotency records. Defaults to
     * resolving the application's discovered entity named
     * 'StripeWebhookEvent'; inject your own entity (mapper-style) to use a
     * different one.
     */
    /**
     * The application's discovered webhook idempotency entity — this sealed
     * package must operate on the entity the app's ORM actually discovered
     * (its own definition, or this package's via the ./persistence subpath).
     * Querying an undiscovered entity object crashes mikro-orm 7.1.x deep in
     * EntityLoader (meta.relations undefined).
     */
    webhookEventEntity: { '~entity': WebhookEventEntity }
  ) {
    this.webhookEventEntity = webhookEventEntity;
    this.partyEnum = partyEnum;
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
   * Resolve the party type for a subscription event.
   * Stripe subscriptions are customer-scoped — customers map to users
   * by default. Override this method to implement organization-level
   * subscriptions or other party resolution logic.
   */
  protected resolvePartyType(_event: Stripe.Event): PartyEnum[keyof PartyEnum] {
    // Default: first value in the enum container.
    // Subclasses can override to inspect event metadata for party type.
    const keys = Object.keys(this.partyEnum as Record<string, unknown>);
    return (this.partyEnum as Record<string, PartyEnum[keyof PartyEnum]>)[
      keys[0]
    ];
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

  /**
   * Our row for a Stripe subscription. Stripe events carry Stripe's
   * subscription id, which is our `externalId`, never our primary key.
   */
  protected findSubscriptionId(
    stripeSubscriptionId: string
  ): Promise<{ id: string } | null> {
    return this.subscriptionService.baseSubscriptionService.findSubscriptionIdByExternalId(
      { externalId: stripeSubscriptionId }
    );
  }

  async handleWebhookEvent(event: Stripe.Event): Promise<void> {
    if (this.openTelemetryCollector) {
      this.openTelemetryCollector.info('Handling webhook event', event);
    }

    if (
      // The Stripe event id is the idempotency key: `request.idempotency_key`
      // is null for every event Stripe originates itself (renewals, failed
      // payments, dashboard edits), and a null in the WHERE clause matched
      // whatever row happened to have none. Only the id is selected, so the
      // check never decrypts the recorded payload and works under any tenant.
      // Querying through the structural shape with an internal cast, the same
      // way the mapper services do (`mapper.entity as typeof Plan`).
      await this.em.findOne(
        this
          .webhookEventEntity as unknown as EntityName<StripeWebhookEventShape>,
        { stripeId: event.id },
        { fields: ['stripeId'] }
      )
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
            partyType: this.resolvePartyType(event),
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
        const existing = await this.findSubscriptionId(event.data.object.id);
        if (!existing) {
          // An update for a subscription we never recorded (the created
          // event was missed, or predates this integration): record it now.
          await this.subscriptionService.baseSubscriptionService.createSubscription(
            {
              partyId:
                typeof event.data.object.customer === 'string'
                  ? event.data.object.customer
                  : event.data.object.customer.id,
              partyType: this.resolvePartyType(event),
              description: event.data.object.description ?? undefined,
              active: isActiveStatus(event.data.object.status),
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
        // The party is the application's to assign (it maps Stripe customers
        // onto its own users or organizations after the fact), so an update
        // never rewrites it.
        await this.subscriptionService.baseSubscriptionService.updateSubscription(
          {
            id: existing.id,
            description: event.data.object.description ?? undefined,
            active: isActiveStatus(event.data.object.status),
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
        const existing = await this.findSubscriptionId(event.data.object.id);
        if (existing) {
          await this.subscriptionService.baseSubscriptionService.deleteSubscription(
            existing
          );
        }
        break;
      }

      case 'customer.subscription.paused': {
        const existing = await this.findSubscriptionId(event.data.object.id);
        if (existing) {
          await this.subscriptionService.baseSubscriptionService.cancelSubscription(
            existing
          );
        }
        break;
      }

      case 'customer.subscription.resumed': {
        const existing = await this.findSubscriptionId(event.data.object.id);
        if (existing) {
          await this.subscriptionService.baseSubscriptionService.resumeSubscription(
            existing
          );
        }
        break;
      }

      default:
        this.openTelemetryCollector.info(
          'Unprocessed stripe event type',
          eventType
        );
        break;
    }

    // em.create (not native em.insert): the app's entity generates its id and
    // timestamps via onCreate hooks, which native inserts bypass — a native
    // insert fails NOT NULL on id for sqlBaseProperties-style entities.
    this.em.create<StripeWebhookEventShape>(
      this.webhookEventEntity as unknown as EntityName<StripeWebhookEventShape>,
      {
        stripeId: event.id,
        idempotencyKey: event.request?.idempotency_key,
        eventType: event.type,
        eventData: event.data
      } as never
    );
    await this.em.flush();
  }
}

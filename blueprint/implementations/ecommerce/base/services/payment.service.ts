import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { PaymentService } from '@forklaunch/interfaces-ecommerce/interfaces';
import {
  ConfirmPaymentDto,
  CreatePaymentDto,
  FailPaymentDto,
  PaymentStatus
} from '@forklaunch/interfaces-ecommerce/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BasePaymentDtos } from '../domain/types/baseEcommerceDto.types';
import { BasePaymentEntities } from '../domain/types/baseEcommerceEntity.types';
import { PaymentMappers } from '../domain/types/payment.mapper.types';
import { Payment } from '../persistence/entities';

/**
 * Persists payment records and their pending/succeeded/failed lifecycle.
 * Provider-specific classes (e.g. StripePaymentService) wrap this and add the
 * actual 3rd-party API calls, delegating persistence back to this base —
 * same split as BasePlanService/StripePlanService in billing.
 */
export class BasePaymentService<
  SchemaValidator extends AnySchemaValidator,
  MapperEntities extends BasePaymentEntities,
  MapperDomains extends BasePaymentDtos = BasePaymentDtos
> implements PaymentService
{
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: PaymentMappers<MapperEntities, MapperDomains>;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: PaymentMappers<MapperEntities, MapperDomains>,
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

  /** Persists a pending payment record. Provider wrappers call the 3rd-party API around this. */
  async createPayment(
    paymentDto: CreatePaymentDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['PaymentMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Creating payment', paymentDto);
    }
    const payment = await this.mappers.CreatePaymentMapper.toEntity(
      paymentDto,
      em ?? this.em,
      ...(args[0] instanceof EntityManager ? args.slice(1) : args)
    );
    await (em ?? this.em).transactional(async (innerEm) => {
      await innerEm.persist(payment);
    });
    return this.mappers.PaymentMapper.toDto(payment);
  }

  async getPayment(
    idDto: { id: string },
    em?: EntityManager
  ): Promise<MapperDomains['PaymentMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting payment', idDto);
    }
    const payment = await (em ?? this.em).findOneOrFail(
      this.mappers.PaymentMapper.entity as typeof Payment,
      idDto
    );
    return this.mappers.PaymentMapper.toDto(
      payment as InferEntity<MapperEntities['PaymentMapper']>
    );
  }

  /** Idempotent: re-confirming an already-succeeded payment is a no-op, not an error. */
  async confirmPayment(
    confirmDto: ConfirmPaymentDto,
    em?: EntityManager
  ): Promise<MapperDomains['PaymentMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Confirming payment', confirmDto);
    }
    const entityManager = em ?? this.em;
    const payment = await entityManager.findOneOrFail(
      this.mappers.PaymentMapper.entity as typeof Payment,
      { providerRef: confirmDto.providerRef }
    );
    if (payment.status !== PaymentStatus.SUCCEEDED) {
      entityManager.assign(payment, { status: PaymentStatus.SUCCEEDED });
      await entityManager.transactional(async (innerEm) => {
        await innerEm.persist(payment);
      });
    }
    return this.mappers.PaymentMapper.toDto(
      payment as InferEntity<MapperEntities['PaymentMapper']>
    );
  }

  async failPayment(
    failDto: FailPaymentDto,
    em?: EntityManager
  ): Promise<MapperDomains['PaymentMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Failing payment', failDto);
    }
    const entityManager = em ?? this.em;
    const payment = await entityManager.findOneOrFail(
      this.mappers.PaymentMapper.entity as typeof Payment,
      { providerRef: failDto.providerRef }
    );
    entityManager.assign(payment, { status: PaymentStatus.FAILED });
    await entityManager.transactional(async (innerEm) => {
      await innerEm.persist(payment);
    });
    return this.mappers.PaymentMapper.toDto(
      payment as InferEntity<MapperEntities['PaymentMapper']>
    );
  }
}

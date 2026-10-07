import { IdDto, RecordTimingDto } from '@forklaunch/common';

export const PaymentStatus = {
  PENDING: 'pending',
  SUCCEEDED: 'succeeded',
  FAILED: 'failed'
} as const;

export type PaymentStatusType = (typeof PaymentStatus)[keyof typeof PaymentStatus];

export type CreatePaymentDto = Partial<IdDto> & {
  orderId: string;
  amountCents: number;
  currency: string;
};

export type PaymentDto = CreatePaymentDto &
  IdDto &
  Partial<RecordTimingDto> & {
    status: PaymentStatusType;
    /** Provider-side reference (e.g. Stripe PaymentIntent id). */
    providerRef?: string;
  };

/**
 * Keyed on providerRef, not id — a webhook only ever has the provider's own
 * reference (e.g. the Stripe PaymentIntent id), never our internal id.
 */
export type ConfirmPaymentDto = {
  providerRef: string;
};

export type FailPaymentDto = {
  providerRef: string;
};

export type PaymentServiceParameters = {
  CreatePaymentDto: CreatePaymentDto;
  PaymentDto: PaymentDto;
  ConfirmPaymentDto: ConfirmPaymentDto;
  FailPaymentDto: FailPaymentDto;
  IdDto: IdDto;
};

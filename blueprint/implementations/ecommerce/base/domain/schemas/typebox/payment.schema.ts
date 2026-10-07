import { enum_, number, optional, string } from '@forklaunch/validator/typebox';

const PaymentStatusEnum = {
  PENDING: 'pending',
  SUCCEEDED: 'succeeded',
  FAILED: 'failed'
} as const;

export const CreatePaymentSchema = {
  orderId: string,
  amountCents: number,
  currency: string
};

export const PaymentSchema = {
  id: string,
  orderId: string,
  amountCents: number,
  currency: string,
  status: enum_(PaymentStatusEnum),
  providerRef: optional(string)
};

export const ConfirmPaymentSchema = {
  providerRef: string
};

export const FailPaymentSchema = {
  providerRef: string
};

export const BasePaymentServiceSchemas = () => ({
  CreatePaymentSchema,
  PaymentSchema
});

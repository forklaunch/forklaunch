import { EntityManager } from '@mikro-orm/core';
import { PaymentServiceParameters } from '../types/payment.service.types';

export interface PaymentService<
  Params extends PaymentServiceParameters = PaymentServiceParameters
> {
  /** Creates a pending payment and initiates it with the provider. */
  createPayment: (
    paymentDto: Params['CreatePaymentDto'],
    em?: EntityManager
  ) => Promise<Params['PaymentDto']>;
  getPayment: (
    idDto: Params['IdDto'],
    em?: EntityManager
  ) => Promise<Params['PaymentDto']>;
  /** Idempotent — driven by the provider's webhook confirming success. */
  confirmPayment: (
    confirmDto: Params['ConfirmPaymentDto'],
    em?: EntityManager
  ) => Promise<Params['PaymentDto']>;
  /** Failed payment — the seam dunning hooks into later. */
  failPayment: (
    failDto: Params['FailPaymentDto'],
    em?: EntityManager
  ) => Promise<Params['PaymentDto']>;
}

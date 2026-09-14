import { isRecord } from '@forklaunch/common';
import type {
  PaypalOrder,
  PaypalOrderStatus
} from '../../services/paypal-client';

const PAYPAL_ORDER_STATUSES: readonly PaypalOrderStatus[] = [
  'CREATED',
  'SAVED',
  'APPROVED',
  'PAYER_ACTION_REQUIRED',
  'VOIDED',
  'COMPLETED'
];

export function isPaypalOrder(x: unknown): x is PaypalOrder {
  return (
    isRecord(x) &&
    typeof x.id === 'string' &&
    typeof x.status === 'string' &&
    (PAYPAL_ORDER_STATUSES as readonly string[]).includes(x.status)
  );
}

export function isAccessTokenResponse(
  x: unknown
): x is { access_token: string } {
  return isRecord(x) && typeof x.access_token === 'string';
}

export function isVerificationResponse(
  x: unknown
): x is { verification_status?: string } {
  return (
    isRecord(x) &&
    (x.verification_status === undefined ||
      typeof x.verification_status === 'string')
  );
}

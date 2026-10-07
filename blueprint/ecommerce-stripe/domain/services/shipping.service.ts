import { ShippingAddressDto } from '@forklaunch/interfaces-ecommerce/types';

export interface ShippingCalculationResult {
  shippingCents: number;
}

export interface ShippingService {
  calculate(params: {
    shippingAddress: ShippingAddressDto;
    subtotalCents: number;
  }): Promise<ShippingCalculationResult>;
}

/**
 * Real flat/table-rate shipping — not a live carrier-rate lookup. This is
 * deliberately the same shape Guild's own shipping-fulfillment guide
 * recommends as the fallback path ("never block checkout on a rate call"),
 * used here as the actual v1 implementation. A live-rate provider (EasyPost/
 * Shippo) is a second ShippingService implementation behind this same
 * interface — checkout.controller.ts never needs to change to add one.
 */
const FREE_SHIPPING_THRESHOLD_CENTS = 5000;
const DOMESTIC_RATE_CENTS = 599;
const INTERNATIONAL_RATE_CENTS = 1999;

export class FlatRateShippingService implements ShippingService {
  async calculate(params: {
    shippingAddress: ShippingAddressDto;
    subtotalCents: number;
  }): Promise<ShippingCalculationResult> {
    if (params.subtotalCents >= FREE_SHIPPING_THRESHOLD_CENTS) {
      return { shippingCents: 0 };
    }
    const isDomestic = params.shippingAddress.country.toUpperCase() === 'US';
    return {
      shippingCents: isDomestic ? DOMESTIC_RATE_CENTS : INTERNATIONAL_RATE_CENTS
    };
  }
}

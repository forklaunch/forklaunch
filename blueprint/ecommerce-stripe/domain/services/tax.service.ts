import { TaxLineDto, ShippingAddressDto } from '@forklaunch/interfaces-ecommerce/types';
import { Metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import Stripe from 'stripe';

export interface TaxCalculationResult {
  taxCents: number;
  breakdown: TaxLineDto[];
  /** True when the real provider couldn't be reached and this is a
   *  configured fallback, not a real jurisdiction-calculated amount — per
   *  Guild's tax-compliance guide: never silently ship $0 tax on a
   *  provider failure, degrade to a flagged estimate instead. */
  estimated: boolean;
}

export interface TaxService {
  calculate(params: {
    lineItemCents: number[];
    shippingAddress: ShippingAddressDto;
  }): Promise<TaxCalculationResult>;
}

/** A flat percentage used only when Stripe Tax itself is unreachable —
 *  deliberately crude (no jurisdiction awareness) because its only job is
 *  to keep checkout from either hard-failing or silently charging $0 tax
 *  while the real provider is down. */
const FALLBACK_TAX_RATE = 0.07;

export class StripeTaxService implements TaxService {
  constructor(
    private stripe: Stripe,
    private openTelemetryCollector: OpenTelemetryCollector<Metrics>
  ) {}

  async calculate(params: {
    lineItemCents: number[];
    shippingAddress: ShippingAddressDto;
  }): Promise<TaxCalculationResult> {
    try {
      const calculation = await this.stripe.tax.calculations.create({
        currency: 'usd',
        line_items: params.lineItemCents.map((amount, i) => ({
          amount,
          reference: `line-${i}`,
          tax_behavior: 'exclusive'
        })),
        customer_details: {
          address: {
            line1: params.shippingAddress.line1,
            line2: params.shippingAddress.line2,
            city: params.shippingAddress.city,
            state: params.shippingAddress.state,
            postal_code: params.shippingAddress.postalCode,
            country: params.shippingAddress.country
          },
          address_source: 'shipping'
        }
      });

      const breakdown: TaxLineDto[] = (calculation.tax_breakdown ?? []).map(
        (line) => ({
          jurisdiction:
            [line.tax_rate_details.state, line.tax_rate_details.country]
              .filter(Boolean)
              .join(', ') || 'unknown',
          taxCents: line.amount
        })
      );

      return {
        taxCents: calculation.tax_amount_exclusive,
        breakdown,
        estimated: false
      };
    } catch (error) {
      // Degrade safely: a real order still needs a real (if approximate)
      // tax line, not $0 — but this must be visible, not silent.
      this.openTelemetryCollector.warn(
        'Stripe Tax unreachable — falling back to a flat estimated rate',
        { error }
      );
      const subtotalCents = params.lineItemCents.reduce((a, b) => a + b, 0);
      const taxCents = Math.round(subtotalCents * FALLBACK_TAX_RATE);
      return {
        taxCents,
        breakdown: [{ jurisdiction: 'estimated-fallback', taxCents }],
        estimated: true
      };
    }
  }
}

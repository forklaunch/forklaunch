/**
 * Minimal PayPal Orders v2 REST client — deliberately no PayPal SDK dependency.
 * A thin fetch wrapper is enough for the v1 payment seam (create + capture),
 * keeps the dependency surface clean, and avoids pulling an SDK the rest of the
 * monorepo doesn't use. Venmo needs no extra code: it rides on PayPal orders as
 * a funding source (US, mobile) exposed automatically at the PayPal checkout.
 */
export interface PaypalOrder {
  id: string;
  status: string;
}

export interface PaypalClientOptions {
  clientId: string;
  clientSecret: string;
  /** e.g. https://api-m.sandbox.paypal.com or https://api-m.paypal.com */
  baseUrl: string;
}

export class PaypalClient {
  private readonly clientId: string;
  private readonly clientSecret: string;
  private readonly baseUrl: string;

  constructor(options: PaypalClientOptions) {
    this.clientId = options.clientId;
    this.clientSecret = options.clientSecret;
    this.baseUrl = options.baseUrl.replace(/\/$/, '');
  }

  private async getAccessToken(): Promise<string> {
    const credentials = Buffer.from(
      `${this.clientId}:${this.clientSecret}`
    ).toString('base64');
    const res = await fetch(`${this.baseUrl}/v1/oauth2/token`, {
      method: 'POST',
      headers: {
        Authorization: `Basic ${credentials}`,
        'Content-Type': 'application/x-www-form-urlencoded'
      },
      body: 'grant_type=client_credentials'
    });
    if (!res.ok) {
      throw new Error(`PayPal auth failed (status ${res.status})`);
    }
    const body = (await res.json()) as { access_token: string };
    return body.access_token;
  }

  /** Creates a PayPal order (money in major units + ISO currency). */
  async createOrder(params: {
    amountCents: number;
    currency: string;
    referenceId: string;
  }): Promise<PaypalOrder> {
    const accessToken = await this.getAccessToken();
    const res = await fetch(`${this.baseUrl}/v2/checkout/orders`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        intent: 'CAPTURE',
        purchase_units: [
          {
            reference_id: params.referenceId,
            amount: {
              currency_code: params.currency.toUpperCase(),
              value: (params.amountCents / 100).toFixed(2)
            }
          }
        ]
      })
    });
    if (!res.ok) {
      throw new Error(`PayPal create order failed (status ${res.status})`);
    }
    return (await res.json()) as PaypalOrder;
  }

  /** Captures a previously-approved order (idempotent on PayPal's side by order id). */
  async captureOrder(orderId: string): Promise<PaypalOrder> {
    const accessToken = await this.getAccessToken();
    const res = await fetch(
      `${this.baseUrl}/v2/checkout/orders/${orderId}/capture`,
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${accessToken}`,
          'Content-Type': 'application/json'
        }
      }
    );
    if (!res.ok) {
      throw new Error(`PayPal capture failed (status ${res.status})`);
    }
    return (await res.json()) as PaypalOrder;
  }
}

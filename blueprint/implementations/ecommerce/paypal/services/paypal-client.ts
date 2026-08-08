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

  /**
   * Verifies a webhook event's transmission signature against PayPal's
   * `/v1/notifications/verify-webhook-signature` API — the officially
   * documented way to validate a PayPal webhook server-side (PayPal does
   * the actual cryptographic verification against its own record of what it
   * sent for `transmissionId`; there's no local public-key/cert-pinning
   * logic to get subtly wrong here). Fails closed: any non-2xx response
   * from PayPal, or any status other than the literal 'SUCCESS', is treated
   * as an unverified event, never as a passed check.
   */
  async verifyWebhookSignature(params: {
    transmissionId: string;
    transmissionTime: string;
    transmissionSig: string;
    certUrl: string;
    authAlgo: string;
    webhookId: string;
    webhookEvent: unknown;
  }): Promise<boolean> {
    const accessToken = await this.getAccessToken();
    const res = await fetch(
      `${this.baseUrl}/v1/notifications/verify-webhook-signature`,
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${accessToken}`,
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({
          transmission_id: params.transmissionId,
          transmission_time: params.transmissionTime,
          cert_url: params.certUrl,
          auth_algo: params.authAlgo,
          transmission_sig: params.transmissionSig,
          webhook_id: params.webhookId,
          webhook_event: params.webhookEvent
        })
      }
    );
    if (!res.ok) {
      return false;
    }
    const body = (await res.json()) as { verification_status?: string };
    return body.verification_status === 'SUCCESS';
  }
}

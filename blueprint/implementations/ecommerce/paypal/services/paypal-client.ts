import {
  isAccessTokenResponse,
  isPaypalOrder,
  isVerificationResponse
} from '../domain/guards/paypal.guards';

/** Order-level status values from PayPal's Orders v2 API. */
export type PaypalOrderStatus =
  | 'CREATED'
  | 'SAVED'
  | 'APPROVED'
  | 'PAYER_ACTION_REQUIRED'
  | 'VOIDED'
  | 'COMPLETED';

export interface PaypalOrder {
  id: string;
  status: PaypalOrderStatus;
}

export interface PaypalClientOptions {
  clientId: string;
  clientSecret: string;
  /** e.g. https://api-m.sandbox.paypal.com or https://api-m.paypal.com */
  baseUrl: string;
}

/** Minimal PayPal Orders v2 REST client (create + capture) — no PayPal SDK. */
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
    const body: unknown = await res.json();
    if (!isAccessTokenResponse(body)) {
      throw new Error(
        'PayPal auth response missing a string access_token field'
      );
    }
    return body.access_token;
  }

  /** Creates a PayPal order (amount in minor units + ISO currency). */
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
    const body: unknown = await res.json();
    if (!isPaypalOrder(body)) {
      throw new Error(
        `PayPal create order returned an unexpected body (reference ${params.referenceId})`
      );
    }
    return body;
  }

  /** Captures a previously-approved order (idempotent by order id). */
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
    const body: unknown = await res.json();
    if (!isPaypalOrder(body)) {
      throw new Error(
        `PayPal capture returned an unexpected body (order ${orderId})`
      );
    }
    return body;
  }

  /** Verifies a webhook signature via PayPal's API. Fails closed. */
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
    const body: unknown = await res.json();
    if (!isVerificationResponse(body)) {
      return false;
    }
    return body.verification_status === 'SUCCESS';
  }
}

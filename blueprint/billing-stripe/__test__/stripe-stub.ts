import { randomBytes } from 'crypto';
import { createServer, IncomingMessage, Server, ServerResponse } from 'http';

/**
 * An in-memory stand-in for the slice of the Stripe API this module calls, so
 * its tests run offline and without a Stripe key. It keeps the objects it
 * creates, answers retrieves, updates, lists and deletes from them, and
 * replies to unknown ids and routes with Stripe's error shape. It is stricter
 * than Stripe about id shape: an id without its Stripe prefix (`plink_`, `cs_`,
 * `sub_`, ...) is refused with a 400, so sending a local id to Stripe fails.
 *
 * Set `STRIPE_LIVE_TESTS=1` (with `STRIPE_API_KEY` set to a Stripe test-mode
 * key) to run the tests against the real Stripe test API instead.
 */
export interface StripeStub {
  /** Stripe client options (`host`, `port`, `protocol`) that reach the stub. */
  readonly clientOptions: { host: string; port: number; protocol: 'http' };
  close(): Promise<void>;
}

type StripeObject = Record<string, unknown> & { id: string; object: string };

type Params = Record<string, unknown>;

/** Decodes Stripe's form encoding (`a[b][0][c]=1`) into nested values. */
function decodeForm(body: string): Params {
  const result: Params = {};
  for (const pair of body.split('&')) {
    if (!pair) continue;
    const [rawKey, rawValue = ''] = pair.split('=');
    const decode = (text: string) =>
      decodeURIComponent(text.replace(/\+/g, ' '));
    const path = decode(rawKey).replace(/\]/g, '').split('[');
    let cursor: Params | unknown[] = result;
    path.forEach((segment, index) => {
      const slot =
        Array.isArray(cursor) && segment === ''
          ? String(cursor.length)
          : segment;
      const holder = cursor as Record<string, unknown>;
      if (index === path.length - 1) {
        holder[slot] = decode(rawValue);
        return;
      }
      if (!isRecord(holder[slot]) && !Array.isArray(holder[slot])) {
        holder[slot] = /^\d*$/.test(path[index + 1]) ? [] : {};
      }
      cursor = holder[slot] as Params | unknown[];
    });
  }
  return result;
}

const now = () => Math.floor(Date.now() / 1000);

const id = (prefix: string) =>
  `${prefix}_test${randomBytes(12).toString('hex')}`;

const toNumber = (value: unknown) =>
  typeof value === 'string' && value !== '' ? Number(value) : undefined;

const toText = (value: unknown, fallback: string) =>
  typeof value === 'string' ? value : fallback;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const list = (url: string, data: unknown[]) => ({
  object: 'list',
  data,
  has_more: false,
  url
});

const base = (prefix: string, object: string, params: Params) => ({
  ...params,
  id: id(prefix),
  object,
  created: now(),
  livemode: false,
  metadata: isRecord(params.metadata) ? params.metadata : {}
});

class StripeError extends Error {
  constructor(
    readonly status: number,
    message: string,
    readonly code?: string
  ) {
    super(message);
  }
}

/**
 * The id prefix Stripe always gives these object types. Plans and products are
 * left out: Stripe lets callers choose their ids.
 */
const ID_PREFIXES: Record<string, string> = {
  payment_methods: 'pm',
  customers: 'cus',
  prices: 'price',
  payment_links: 'plink',
  checkout_sessions: 'cs',
  billing_portal_sessions: 'bps',
  subscriptions: 'sub'
};

let running: StripeStub | undefined;

/** The stub started by {@link startStripeStub}, until it is closed. */
export const runningStripeStub = (): StripeStub | undefined => running;

export async function startStripeStub(): Promise<StripeStub> {
  const store = new Map<string, Map<string, StripeObject>>();
  const table = (name: string) => {
    let entries = store.get(name);
    if (!entries) {
      entries = new Map();
      store.set(name, entries);
    }
    return entries;
  };
  const find = (name: string, object: string, key: string) => {
    // Stripe ids carry a type prefix. Refuse anything else outright, so a
    // service that sends a local (public UUID) id to Stripe fails loudly here
    // instead of only failing against the real API.
    const prefix = ID_PREFIXES[name];
    if (prefix && !new RegExp(`^${prefix}_[A-Za-z0-9_]+$`).test(key)) {
      throw new StripeError(
        400,
        `Invalid ${object} id '${key}': Stripe ${object} ids start with '${prefix}_'`,
        'invalid_stripe_id'
      );
    }
    const found = table(name).get(key);
    if (!found) {
      throw new StripeError(
        404,
        `No such ${object}: '${key}'`,
        'resource_missing'
      );
    }
    return found;
  };
  const save = (name: string, value: StripeObject) => {
    table(name).set(value.id, value);
    return value;
  };
  const merge = (target: StripeObject, params: Params) => {
    const { metadata, ...rest } = params;
    Object.assign(target, rest);
    if (isRecord(metadata)) {
      target.metadata = {
        ...(isRecord(target.metadata) ? target.metadata : {}),
        ...metadata
      };
    }
    return target;
  };

  const plan = (params: Params): StripeObject => ({
    ...base('plan', 'plan', params),
    active: true,
    amount: toNumber(params.amount) ?? null,
    currency: toText(params.currency, 'usd').toLowerCase(),
    interval: params.interval ?? 'month',
    interval_count: toNumber(params.interval_count) ?? 1,
    product: params.product ?? null
  });

  const subscriptionItems = (subscriptionId: string, items: unknown) =>
    list(
      `/v1/subscription_items?subscription=${subscriptionId}`,
      (Array.isArray(items) ? items : []).filter(isRecord).map((item) => {
        const planId = toText(item.plan ?? item.price, '');
        const planObject = planId
          ? (table('plans').get(planId) ?? { id: planId, object: 'plan' })
          : undefined;
        return {
          id: toText(item.id, id('si')),
          object: 'subscription_item',
          plan: planObject,
          price: planObject
            ? { id: planObject.id, object: 'price' }
            : undefined,
          quantity: toNumber(item.quantity) ?? 1,
          subscription: subscriptionId
        };
      })
    );

  const expand = (value: StripeObject, expansions: string[]) => {
    if (expansions.includes('product') && typeof value.product === 'string') {
      return {
        ...value,
        product: table('products').get(value.product) ?? {
          id: value.product,
          object: 'product'
        }
      };
    }
    return value;
  };

  const handle = (
    method: string,
    path: string,
    params: Params,
    expansions: string[]
  ): unknown => {
    const segments = path.replace(/^\/v1\//, '').split('/');
    const route = `${method} ${segments
      .map((segment, index) =>
        index > 0 &&
        !['line_items', 'expire', 'resume', 'search', 'sessions'].includes(
          segment
        )
          ? ':id'
          : segment
      )
      .join('/')}`;
    const key =
      segments[0] === 'checkout' || segments[0] === 'billing_portal'
        ? segments[2]
        : segments[1];

    switch (route) {
      case 'POST payment_methods':
        return save('payment_methods', {
          ...base('pm', 'payment_method', params),
          type: params.type ?? 'card',
          card: {
            brand: 'visa',
            last4: '4242',
            exp_month: 12,
            exp_year: 2034
          }
        });
      case 'POST customers':
        return save('customers', base('cus', 'customer', params));
      case 'GET customers/:id':
        return find('customers', 'customer', key);

      case 'POST products':
        return save('products', {
          ...base('prod', 'product', params),
          active: true
        });
      case 'GET products/:id':
        return find('products', 'product', key);
      case 'POST products/:id':
        return merge(find('products', 'product', key), params);
      case 'GET products/search':
        return {
          ...list('/v1/products/search', [...table('products').values()]),
          object: 'search_result'
        };

      case 'POST prices':
        return save('prices', {
          ...base('price', 'price', params),
          active: true,
          currency: toText(params.currency, 'usd').toLowerCase(),
          unit_amount: toNumber(params.unit_amount) ?? null,
          type: params.recurring ? 'recurring' : 'one_time'
        });
      case 'GET prices':
        return list('/v1/prices', [...table('prices').values()]);
      case 'GET prices/:id':
        return find('prices', 'price', key);
      case 'POST prices/:id':
        return merge(find('prices', 'price', key), params);

      case 'POST plans':
        return save('plans', plan(params));
      case 'GET plans/:id':
        return expand(find('plans', 'plan', key), expansions);
      case 'DELETE plans/:id':
        find('plans', 'plan', key);
        table('plans').delete(key);
        return { id: key, object: 'plan', deleted: true };

      case 'POST payment_links': {
        const created = base('plink', 'payment_link', params);
        return save('payment_links', {
          ...created,
          active: true,
          url: `https://buy.stripe.com/test_${created.id}`
        });
      }
      case 'GET payment_links':
        return list(
          '/v1/payment_links',
          [...table('payment_links').values()].filter(
            (link) =>
              params.active === undefined ||
              String(link.active) === toText(params.active, '')
          )
        );
      case 'GET payment_links/:id':
        return find('payment_links', 'payment_link', key);
      case 'POST payment_links/:id':
        return merge(find('payment_links', 'payment_link', key), params);
      case 'GET payment_links/:id/line_items':
        find('payment_links', 'payment_link', key);
        return list(`/v1/payment_links/${key}/line_items`, []);

      case 'POST checkout/sessions': {
        const created = base('cs', 'checkout.session', params);
        return save('checkout_sessions', {
          ...created,
          // Stripe reports an absent customer as null, not a missing key.
          customer: params.customer ?? null,
          status: 'open',
          payment_status: 'unpaid',
          expires_at: now() + 24 * 60 * 60,
          url: `https://checkout.stripe.com/c/pay/${created.id}`
        });
      }
      case 'GET checkout/sessions/:id':
        return find('checkout_sessions', 'checkout.session', key);
      case 'POST checkout/sessions/:id':
        return merge(
          find('checkout_sessions', 'checkout.session', key),
          params
        );
      case 'POST checkout/sessions/:id/expire':
        return merge(find('checkout_sessions', 'checkout.session', key), {
          status: 'expired',
          url: null
        });

      case 'POST billing_portal/sessions': {
        const created = base('bps', 'billing_portal.session', params);
        return save('billing_portal_sessions', {
          ...created,
          url: `https://billing.stripe.com/p/session/${created.id}`
        });
      }

      case 'POST subscriptions': {
        const created = base('sub', 'subscription', params);
        const start = now();
        return save('subscriptions', {
          ...created,
          status: 'active',
          cancel_at_period_end: false,
          current_period_start: start,
          current_period_end: start + 30 * 24 * 60 * 60,
          start_date: start,
          items: subscriptionItems(created.id, params.items)
        });
      }
      case 'GET subscriptions/:id':
        return find('subscriptions', 'subscription', key);
      case 'POST subscriptions/:id': {
        const existing = find('subscriptions', 'subscription', key);
        const { items, ...rest } = params;
        merge(existing, rest);
        if (items !== undefined) {
          existing.items = subscriptionItems(key, items);
        }
        return existing;
      }
      case 'DELETE subscriptions/:id':
        return merge(find('subscriptions', 'subscription', key), {
          status: 'canceled',
          canceled_at: now()
        });
      case 'POST subscriptions/:id/resume':
        return merge(find('subscriptions', 'subscription', key), {
          status: 'active',
          canceled_at: null
        });

      default:
        throw new StripeError(
          404,
          `Unrecognized request URL (${method}: ${path}).`
        );
    }
  };

  const server: Server = createServer(
    (req: IncomingMessage, res: ServerResponse) => {
      let body = '';
      req.setEncoding('utf8');
      req.on('data', (chunk: string) => (body += chunk));
      req.on('end', () => {
        const url = new URL(req.url ?? '/', 'http://localhost');
        const query = decodeForm(url.search.replace(/^\?/, ''));
        const form = decodeForm(body);
        const params = { ...query, ...form };
        const expansions = Array.isArray(params.expand)
          ? params.expand.filter((x): x is string => typeof x === 'string')
          : [];
        delete params.expand;
        try {
          const result = handle(
            req.method ?? 'GET',
            url.pathname,
            params,
            expansions
          );
          res.writeHead(200, {
            'content-type': 'application/json',
            'request-id': id('req')
          });
          res.end(JSON.stringify(result));
        } catch (error) {
          const status = error instanceof StripeError ? error.status : 500;
          res.writeHead(status, { 'content-type': 'application/json' });
          res.end(
            JSON.stringify({
              error: {
                type: status === 500 ? 'api_error' : 'invalid_request_error',
                code: error instanceof StripeError ? error.code : undefined,
                message: error instanceof Error ? error.message : 'Unknown error'
              }
            })
          );
        }
      });
    }
  );
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve());
  });
  server.unref();
  const address = server.address();
  if (!address || typeof address === 'string') {
    throw new Error('The Stripe stub is not listening on a TCP port');
  }
  const { port } = address;

  const stub: StripeStub = {
    clientOptions: { host: '127.0.0.1', port, protocol: 'http' },
    close: () =>
      new Promise<void>((resolve) => {
        if (running === stub) running = undefined;
        server.closeAllConnections?.();
        server.close(() => resolve());
      })
  };
  running = stub;
  return stub;
}

# Catalog import contract — `POST /catalog-import`

This documents the one API surface a migration tool loads a catalog through.
The ecommerce module (`blueprint/ecommerce-stripe`) owns the endpoint; this
tool, and any other migration tool, is a client of it. This doc is what a
client needs to know.

A working reference implementation of everything below (pull → normalize →
import, including HMAC signing and batching) is `scripts/catalog/cli.ts` in
this folder, safe to read end-to-end alongside this doc.

## The endpoint

```
POST /catalog-import
Content-Type: application/json
Authorization: HMAC keyId=<id> ts=<iso8601> nonce=<uuid> signature=<base64>
```

Request body:

```ts
{
  products: Array<{
    externalId: string;          // required — see "Idempotency" below
    handle: string;               // required — URL slug
    sourceUrl?: string;
    title: string;                // required
    descriptionHtml?: string;
    vendor?: string;
    productType?: string;
    tags?: string[];
    options?: Array<{ name: string; isPackQuantity: boolean; values: string[] }>;
    images?: Array<{ src: string; position: number }>;
    variants: Array<{            // required, min 1
      externalId: string;         // required — separate ID from the product's
      sku?: string;
      title: string;              // required
      optionValues?: Record<string, string>;
      priceCents: number;         // required — integer cents, never floats
      compareAtPriceCents?: number;
      requiresShipping?: boolean;
      initialStock?: number;      // seed value, defaults to 0 if omitted
    }>;
  }>;
}
```

Response: `200 { productsImported: number, variantsImported: number }` — a
running count of products/variants that made it through, whether newly
created or updated on this call.

## Products vs. variants have separate IDs — don't collapse them

A "product" (e.g. a t-shirt) and each of its variants (Small/Red, Small/Blue,
Large/Red...) are different entities with **independently assigned IDs** on
the source platform. Send both — `product.externalId` for the product, and
`variant.externalId` for each variant, distinct from `variant.sku` (which is
a merchant-typed label, optional, and not reliable for identity — plenty of
merchants leave it blank or reuse it).

## Idempotency — safe to retry, by design

Loading is keyed on `externalId`, at both the product and variant level.
Re-running an import with the same data **upserts, never duplicates** — this
is true today, verified against a real Postgres-backed run (52 products / 141
variants imported twice, zero duplicates both times). Retry freely on any
failure; there is no need to track what already succeeded before retrying.

Inventory is only seeded once, on first creation of a variant — a re-import
never resets live stock counts back to a stale snapshot.

## Request size — batch your calls

The server's JSON body limit is 100KB. A real store's full catalog will
almost certainly exceed that in one request (a 52-product real catalog was
~286KB). **Chunk `products` into batches** — the reference implementation
uses 10 products per request — and call the endpoint once per batch,
accumulating the returned counts. This also matches the batched/resumable
job model your own `commerce-migration` skill guide already specifies.

## The HMAC signing gotcha — read this before you debug a 403

**The signature must be computed over the path as the route handler sees it,
not the external request path.** This module's routers are mounted at a
prefix (e.g. `/catalog-import`) with the handler registered at `/` inside
that router — the framework rewrites the request path to be relative to the
mount before your handler (and the HMAC verification step) ever sees it. So:

- Request URL: `POST https://<server>/catalog-import`
- Path used **only for computing the signature**: `/`

Signing the wrong path produces the exact same error as a wrong secret key —
`403 Invalid Authorization signature` — with no way to tell the two apart
from the response alone. If you get a 403 and the secret is definitely
correct, this is almost certainly why. The reference implementation's
`signHmac()` in `scripts/catalog/cli.ts` documents and handles this correctly —
use it as-is rather than re-deriving it.

Signature construction (matches `@forklaunch/core`'s `createHmacToken`):

```
message = `${method}\n${signedPath}\n${body ? JSON.stringify(body) + '\n' : ''}${timestamp.toISOString()}\n${nonce}`
signature = base64(HMAC-SHA256(secretKey, message))
header = `HMAC keyId=default ts=${timestamp.toISOString()} nonce=${nonce} signature=${signature}`
```

## What's out of scope for this endpoint

- Customer and historical order migration — not built yet on our side; this
  endpoint is catalog-only today.
- Media re-hosting — image `src` URLs are stored as given; re-pointing them
  at your own CDN (per your `commerce-migration` guide's "don't hotlink the
  old store" guidance) is your tool's responsibility, not this endpoint's.
- Parity verification / cutover — this endpoint only loads data; the
  verify-before-cutover gate your migration pipeline needs is built on your
  side, reading back through this module's normal list/get endpoints.

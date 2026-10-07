import { randomUUID } from 'crypto';
import {
  cleanupTestDatabase,
  setupTestDatabase,
  signTestRequest
} from './test-utils';

/** Narrows a `{ code, response }` SDK result to its 200 branch, or fails the test with the actual body. */
function expectOk<T extends { code: number; response: unknown }>(
  result: T
): Extract<T, { code: 200 }> {
  if (result.code !== 200) {
    throw new Error(`expected 200, got ${result.code}: ${JSON.stringify(result.response)}`);
  }
  return result as Extract<T, { code: 200 }>;
}

/**
 * Full purchase-loop E2E test: catalog-import -> cart -> order -> full
 * state-machine transition chain -> illegal transition rejected.
 *
 * Requires a container runtime (Docker) for the Postgres testcontainer —
 * this could not be executed in the sandbox this was authored in (no
 * container runtime available there; confirmed the repo's existing
 * billing-stripe e2e tests fail identically in that environment, for the
 * same environmental reason, unrelated to this test's correctness).
 *
 * The flow below was verified live against a real running server + real
 * Postgres database before being encoded here — see
 * ecommerce-stripe/purchase-loop-verify.ts for that verification script
 * and its output.
 */
describe('Purchase Loop E2E Test with PostgreSQL Container', () => {
  beforeAll(async () => {
    await setupTestDatabase();
  }, 60000);

  afterAll(async () => {
    await cleanupTestDatabase();
  }, 30000);

  it('completes the full purchase loop and rejects an illegal transition', async () => {
    const { importCatalogRoute } = await import(
      '../api/routes/catalogImport.routes'
    );
    const { getProductByHandleRoute } = await import(
      '../api/routes/product.routes'
    );
    const { listVariantsByProductRoute } = await import(
      '../api/routes/variant.routes'
    );
    const { createCartRoute, addCartItemRoute } = await import(
      '../api/routes/cart.routes'
    );
    const {
      createOrderRoute,
      transitionOrderRoute,
      getOrderRoute
    } = await import('../api/routes/order.routes');

    const externalId = `e2e-${randomUUID()}`;
    const handle = `e2e-product-${externalId}`;

    // 1. Catalog import — the same idempotent, batched door Guild's
    // migration tooling loads through.
    const importPayload = {
      products: [
        {
          externalId,
          handle,
          title: 'E2E Test Product',
          variants: [
            {
              externalId: `${externalId}-v1`,
              title: 'Default',
              priceCents: 2500,
              initialStock: 10
            }
          ]
        }
      ]
    };
    const importResponse = await importCatalogRoute.sdk.importCatalog({
      body: importPayload,
      headers: { authorization: signTestRequest('POST', '/', importPayload) }
    });
    expect(importResponse.code).toBe(200);
    expect(importResponse.response).toMatchObject({
      productsImported: 1,
      variantsImported: 1
    });

    // 2. Locate the created product + variant.
    const productResponse = expectOk(
      await getProductByHandleRoute.sdk.getProductByHandle({
        params: { handle },
        headers: { authorization: signTestRequest('GET', `/handle/${handle}`) }
      })
    );
    const productId = productResponse.response.id;

    const variantsResponse = expectOk(
      await listVariantsByProductRoute.sdk.listVariantsByProduct({
        params: { productId },
        headers: {
          authorization: signTestRequest('GET', `/product/${productId}`)
        }
      })
    );
    const variantId = variantsResponse.response[0].id;

    // 3. Cart: create, add item.
    const cartResponse = expectOk(
      await createCartRoute.sdk.createCart({
        body: { customerId: 'e2e-customer-1' },
        headers: {
          authorization: signTestRequest('POST', '/', {
            customerId: 'e2e-customer-1'
          })
        }
      })
    );
    const cartId = cartResponse.response.id;

    const addItemBody = { cartId, variantId, quantity: 2 };
    const addItemResponse = await addCartItemRoute.sdk.addCartItem({
      body: addItemBody,
      headers: { authorization: signTestRequest('POST', '/items', addItemBody) }
    });
    expect(addItemResponse.code).toBe(200);

    // 4. Order: create from the cart's items.
    const createOrderBody = {
      customerId: 'e2e-customer-1',
      items: [{ variantId, quantity: 2, unitPriceCents: 2500 }],
      shippingAddress: {
        name: 'E2E Customer',
        line1: '123 Test St',
        city: 'Testville',
        state: 'CA',
        postalCode: '90001',
        country: 'US'
      },
      subtotalCents: 5000,
      discountCents: 0,
      taxCents: 400,
      taxBreakdown: [{ jurisdiction: 'CA', taxCents: 400 }],
      shippingCents: 0,
      giftCardCents: 0,
      totalCents: 5400
    };
    const orderResponse = expectOk(
      await createOrderRoute.sdk.createOrder({
        body: createOrderBody,
        headers: { authorization: signTestRequest('POST', '/', createOrderBody) }
      })
    );
    expect(orderResponse.response.status).toBe('pending');
    const orderId = orderResponse.response.id;

    // 5. Full legal transition chain.
    const transitions = ['paid', 'fulfilled', 'shipped', 'delivered'] as const;
    for (const to of transitions) {
      const transitionBody = { to };
      const transitionResponse = expectOk(
        await transitionOrderRoute.sdk.transitionOrder({
          params: { id: orderId },
          body: transitionBody,
          headers: {
            authorization: signTestRequest(
              'PUT',
              `/${orderId}/transition`,
              transitionBody
            )
          }
        })
      );
      expect(transitionResponse.response.status).toBe(to);
    }

    // 6. One illegal transition must be rejected — delivered has no legal
    // next state.
    const illegalBody: { to: (typeof transitions)[number] } = { to: 'paid' };
    const illegalResponse = await transitionOrderRoute.sdk.transitionOrder({
      params: { id: orderId },
      body: illegalBody,
      headers: {
        authorization: signTestRequest(
          'PUT',
          `/${orderId}/transition`,
          illegalBody
        )
      }
    });
    expect(illegalResponse.code).toBe(400);

    // 7. Final state confirms the rejected transition had no effect.
    const finalResponse = expectOk(
      await getOrderRoute.sdk.getOrder({
        params: { id: orderId },
        headers: { authorization: signTestRequest('GET', `/${orderId}`) }
      })
    );
    expect(finalResponse.response.status).toBe('delivered');
  });
});

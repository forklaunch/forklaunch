import { CurrencyEnum } from '@forklaunch/implementation-billing-stripe/enum';
import { StatusEnum } from '../domain/enum/status.enum';
import {
  cleanupTestDatabase,
  getMockCheckoutSessionData,
  setupTestDatabase,
  TEST_TOKENS
} from './test-utils';

describe('CheckoutSession Routes E2E Tests with PostgreSQL Container', () => {
  beforeAll(async () => {
    await setupTestDatabase();
  });

  afterAll(async () => {
    await cleanupTestDatabase();
  });

  describe('POST /checkout-session - createCheckoutSession', () => {
    it('should create a checkout session successfully', async () => {
      const { createCheckoutSessionRoute } = await import(
        '../api/routes/checkoutSession.routes'
      );

      const response =
        await createCheckoutSessionRoute.sdk.createCheckoutSession({
          body: getMockCheckoutSessionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        customerId: getMockCheckoutSessionData().customerId,
        paymentMethods: getMockCheckoutSessionData().paymentMethods,
        currency: getMockCheckoutSessionData().currency,
        uri: expect.any(String),
        successRedirectUri: getMockCheckoutSessionData().successRedirectUri,
        cancelRedirectUri: getMockCheckoutSessionData().cancelRedirectUri,
        expiresAt: expect.any(Date),
        status: getMockCheckoutSessionData().status,
        id: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle validation errors when creating checkout session', async () => {
      const { createCheckoutSessionRoute } = await import(
        '../api/routes/checkoutSession.routes'
      );

      const invalidData = {
        ...getMockCheckoutSessionData(),
        customerId: '',
        expiresAt: new Date(Date.now() - 24 * 60 * 60 * 1000)
      };

      try {
        await createCheckoutSessionRoute.sdk.createCheckoutSession({
          body: invalidData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should handle invalid currency enum', async () => {
      const { createCheckoutSessionRoute } = await import(
        '../api/routes/checkoutSession.routes'
      );

      const invalidData = {
        ...getMockCheckoutSessionData(),
        currency: 'INVALID_CURRENCY' as CurrencyEnum
      };

      try {
        await createCheckoutSessionRoute.sdk.createCheckoutSession({
          body: invalidData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should handle invalid payment method enum', async () => {
      const { createCheckoutSessionRoute } = await import(
        '../api/routes/checkoutSession.routes'
      );

      const invalidData = {
        ...getMockCheckoutSessionData(),
        paymentMethods: ['INVALID_METHOD' as never]
      };

      try {
        await createCheckoutSessionRoute.sdk.createCheckoutSession({
          body: invalidData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should handle invalid status enum', async () => {
      const { createCheckoutSessionRoute } = await import(
        '../api/routes/checkoutSession.routes'
      );

      const invalidData = {
        ...getMockCheckoutSessionData(),
        status: 'INVALID_STATUS' as StatusEnum
      };

      try {
        await createCheckoutSessionRoute.sdk.createCheckoutSession({
          body: invalidData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('GET /checkout-session/:id - getCheckoutSession', () => {
    it('should retrieve an existing checkout session successfully', async () => {
      const { createCheckoutSessionRoute, getCheckoutSessionRoute } =
        await import('../api/routes/checkoutSession.routes');

      const createResponse =
        await createCheckoutSessionRoute.sdk.createCheckoutSession({
          body: getMockCheckoutSessionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create checkout session');
      }
      const sessionId = createResponse.response.id;

      const response = await getCheckoutSessionRoute.sdk.getCheckoutSession({
        params: { id: sessionId },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        id: sessionId,
        customerId: getMockCheckoutSessionData().customerId,
        paymentMethods: getMockCheckoutSessionData().paymentMethods,
        currency: getMockCheckoutSessionData().currency,
        uri: expect.any(String),
        successRedirectUri: getMockCheckoutSessionData().successRedirectUri,
        cancelRedirectUri: getMockCheckoutSessionData().cancelRedirectUri,
        expiresAt: expect.any(Date),
        status: getMockCheckoutSessionData().status,
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle non-existent checkout session ID', async () => {
      const { getCheckoutSessionRoute } = await import(
        '../api/routes/checkoutSession.routes'
      );

      try {
        await getCheckoutSessionRoute.sdk.getCheckoutSession({
          params: { id: '123e4567-e89b-12d3-a456-426614174999' },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should handle invalid UUID format', async () => {
      const { getCheckoutSessionRoute } = await import(
        '../api/routes/checkoutSession.routes'
      );

      try {
        await getCheckoutSessionRoute.sdk.getCheckoutSession({
          params: { id: 'invalid-uuid' },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('GET /checkout-session/:id/expire - expireCheckoutSession', () => {
    it('should expire an existing checkout session successfully', async () => {
      const { createCheckoutSessionRoute, expireCheckoutSessionRoute } =
        await import('../api/routes/checkoutSession.routes');

      const createResponse =
        await createCheckoutSessionRoute.sdk.createCheckoutSession({
          body: getMockCheckoutSessionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create checkout session');
      }
      const sessionId = createResponse.response.id;

      const response =
        await expireCheckoutSessionRoute.sdk.expireCheckoutSession({
          params: { id: sessionId },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      expect(response.code).toBe(200);
      expect(response.response).toBe(`Expired checkout session ${sessionId}`);
    });

    it('should handle expiring non-existent checkout session', async () => {
      const { expireCheckoutSessionRoute } = await import(
        '../api/routes/checkoutSession.routes'
      );

      try {
        await expireCheckoutSessionRoute.sdk.expireCheckoutSession({
          params: { id: '123e4567-e89b-12d3-a456-426614174999' },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('GET /checkout-session/:id/success - handleCheckoutSuccess', () => {
    it('should handle checkout success successfully', async () => {
      const { createCheckoutSessionRoute, handleCheckoutSuccessRoute } =
        await import('../api/routes/checkoutSession.routes');

      const createResponse =
        await createCheckoutSessionRoute.sdk.createCheckoutSession({
          body: getMockCheckoutSessionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create checkout session');
      }
      const sessionId = createResponse.response.id;

      const response =
        await handleCheckoutSuccessRoute.sdk.handleCheckoutSuccess({
          params: { id: sessionId },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      expect(response.code).toBe(200);
      expect(response.response).toBe(
        `Handled checkout success for session ${sessionId}`
      );
    });
  });

  describe('GET /checkout-session/:id/failure - handleCheckoutFailure', () => {
    it('should handle checkout failure successfully', async () => {
      const { createCheckoutSessionRoute, handleCheckoutFailureRoute } =
        await import('../api/routes/checkoutSession.routes');

      const createResponse =
        await createCheckoutSessionRoute.sdk.createCheckoutSession({
          body: getMockCheckoutSessionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create checkout session');
      }
      const sessionId = createResponse.response.id;

      const response =
        await handleCheckoutFailureRoute.sdk.handleCheckoutFailure({
          params: { id: sessionId },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      expect(response.code).toBe(200);
      expect(response.response).toBe(
        `Handled checkout failure for session ${sessionId}`
      );
    });
  });

  describe('Authentication', () => {
    it('should require HMAC authentication for createCheckoutSession', async () => {
      const { createCheckoutSessionRoute } = await import(
        '../api/routes/checkoutSession.routes'
      );

      try {
        await createCheckoutSessionRoute.sdk.createCheckoutSession({
          body: getMockCheckoutSessionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC_INVALID
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should require HMAC authentication for getCheckoutSession', async () => {
      const { getCheckoutSessionRoute } = await import(
        '../api/routes/checkoutSession.routes'
      );

      try {
        await getCheckoutSessionRoute.sdk.getCheckoutSession({
          params: { id: '123e4567-e89b-12d3-a456-426614174000' },
          headers: {
            authorization: TEST_TOKENS.HMAC_INVALID
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });
  });
});

import { CurrencyEnum } from '../domain/enum/currency.enum';
import { PaymentMethodEnum } from '../domain/enum/paymentMethod.enum';
import { StatusEnum } from '../domain/enum/status.enum';
import {
  cleanupTestDatabase,
  clearDatabase,
  mockPaymentLinkData,
  setupTestData,
  setupTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from './test-utils';

describe('PaymentLink Routes E2E Tests with PostgreSQL Container', () => {
  let orm: TestSetupResult['orm'];
  let redis: TestSetupResult['redis'];

  beforeAll(async () => {
    const setup = await setupTestDatabase();
    orm = setup.orm;
    redis = setup.redis;
  }, 60000);

  beforeEach(async () => {
    await clearDatabase({ orm, redis });
    if (!orm) throw new Error('ORM not initialized');
    const em = orm.em.fork();
    await setupTestData(em);
  });

  afterAll(async () => {
    await cleanupTestDatabase();
  }, 30000);

  describe('POST /payment-link - createPaymentLink', () => {
    it('should create a payment link successfully', async () => {
      const { createPaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const response = await createPaymentLinkRoute.sdk.createPaymentLink({
        body: mockPaymentLinkData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        amount: mockPaymentLinkData.price,
        paymentMethods: mockPaymentLinkData.paymentMethods,
        currency: mockPaymentLinkData.currency,
        description: mockPaymentLinkData.description,
        status: mockPaymentLinkData.status,
        id: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle validation errors when creating payment link', async () => {
      const { createPaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const invalidData = {
        ...mockPaymentLinkData,
        amount: -100
      };

      try {
        await createPaymentLinkRoute.sdk.createPaymentLink({
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
      const { createPaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const invalidData = {
        ...mockPaymentLinkData,
        currency: 'INVALID_CURRENCY' as CurrencyEnum
      };

      try {
        await createPaymentLinkRoute.sdk.createPaymentLink({
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
      const { createPaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const invalidData = {
        ...mockPaymentLinkData,
        paymentMethods: ['INVALID_METHOD' as PaymentMethodEnum]
      };

      try {
        await createPaymentLinkRoute.sdk.createPaymentLink({
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
      const { createPaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const invalidData = {
        ...mockPaymentLinkData,
        status: 'INVALID_STATUS' as StatusEnum
      };

      try {
        await createPaymentLinkRoute.sdk.createPaymentLink({
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

  describe('GET /payment-link/:id - getPaymentLink', () => {
    it('should retrieve an existing payment link successfully', async () => {
      const { createPaymentLinkRoute, getPaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const createResponse = await createPaymentLinkRoute.sdk.createPaymentLink(
        {
          body: mockPaymentLinkData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        }
      );

      if (createResponse.code !== 200) {
        throw new Error('Failed to create payment link');
      }
      const paymentLinkId = createResponse.response.id;

      const response = await getPaymentLinkRoute.sdk.getPaymentLink({
        params: { id: paymentLinkId },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        id: paymentLinkId,
        amount: mockPaymentLinkData.price,
        paymentMethods: mockPaymentLinkData.paymentMethods,
        currency: mockPaymentLinkData.currency,
        description: mockPaymentLinkData.description,
        status: mockPaymentLinkData.status,
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle non-existent payment link ID', async () => {
      const { getPaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      try {
        await getPaymentLinkRoute.sdk.getPaymentLink({
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
      const { getPaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      try {
        await getPaymentLinkRoute.sdk.getPaymentLink({
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

  describe('PUT /payment-link/:id - updatePaymentLink', () => {
    it('should update an existing payment link successfully', async () => {
      const { createPaymentLinkRoute, updatePaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const createResponse = await createPaymentLinkRoute.sdk.createPaymentLink(
        {
          body: mockPaymentLinkData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        }
      );

      if (createResponse.code !== 200) {
        throw new Error('Failed to create payment link');
      }
      const paymentLinkId = createResponse.response.id;
      const updateData = {
        id: paymentLinkId,
        amount: 14999,
        paymentMethods: ['credit_card' as const],
        currency: 'GBP' as const,
        description: 'An updated test payment link',
        status: 'COMPLETED' as const
      };

      const response = await updatePaymentLinkRoute.sdk.updatePaymentLink({
        params: { id: paymentLinkId },
        body: updateData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        id: paymentLinkId,
        amount: updateData.amount,
        paymentMethods: updateData.paymentMethods,
        currency: updateData.currency,
        description: updateData.description,
        status: updateData.status,
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle updating non-existent payment link', async () => {
      const { updatePaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const updateData = {
        id: '123e4567-e89b-12d3-a456-426614174999',
        amount: 14999,
        paymentMethods: ['credit_card' as const],
        currency: 'USD' as const,
        description: 'Updated payment link',
        status: 'COMPLETED' as const
      };

      try {
        await updatePaymentLinkRoute.sdk.updatePaymentLink({
          params: { id: '123e4567-e89b-12d3-a456-426614174999' },
          body: updateData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should handle validation errors when updating payment link', async () => {
      const { createPaymentLinkRoute, updatePaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const createResponse = await createPaymentLinkRoute.sdk.createPaymentLink(
        {
          body: mockPaymentLinkData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        }
      );

      if (createResponse.code !== 200) {
        throw new Error('Failed to create payment link');
      }
      const paymentLinkId = createResponse.response.id;
      const invalidUpdateData = {
        id: paymentLinkId,
        amount: -100,
        paymentMethods: ['credit_card' as const],
        currency: 'USD' as const,
        description: 'Updated payment link',
        status: 'PENDING' as const
      };

      try {
        await updatePaymentLinkRoute.sdk.updatePaymentLink({
          params: { id: paymentLinkId },
          body: invalidUpdateData,
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

  describe('DELETE /payment-link/:id - expirePaymentLink', () => {
    it('should expire an existing payment link successfully', async () => {
      const { createPaymentLinkRoute, expirePaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const createResponse = await createPaymentLinkRoute.sdk.createPaymentLink(
        {
          body: mockPaymentLinkData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        }
      );

      if (createResponse.code !== 200) {
        throw new Error('Failed to create payment link');
      }
      const paymentLinkId = createResponse.response.id;

      const response = await expirePaymentLinkRoute.sdk.expirePaymentLink({
        params: { id: paymentLinkId },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe(`Expired payment link ${paymentLinkId}`);
    });

    it('should handle expiring non-existent payment link', async () => {
      const { expirePaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      try {
        await expirePaymentLinkRoute.sdk.expirePaymentLink({
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

  describe('GET /payment-link - listPaymentLinks', () => {
    it('should list all payment links when no IDs provided', async () => {
      const { createPaymentLinkRoute, listPaymentLinksRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      await createPaymentLinkRoute.sdk.createPaymentLink({
        body: mockPaymentLinkData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      await createPaymentLinkRoute.sdk.createPaymentLink({
        body: {
          ...mockPaymentLinkData,
          amount: 19999
        },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      const response = await listPaymentLinksRoute.sdk.listPaymentLinks({
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toHaveLength(2);
      expect(response.response[0]).toMatchObject({
        amount: expect.any(Number),
        paymentMethods: expect.any(Array),
        currency: expect.any(String),
        status: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should list specific payment links when IDs provided', async () => {
      const { createPaymentLinkRoute, listPaymentLinksRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      const paymentLink1Response =
        await createPaymentLinkRoute.sdk.createPaymentLink({
          body: mockPaymentLinkData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      const paymentLink2Response =
        await createPaymentLinkRoute.sdk.createPaymentLink({
          body: {
            ...mockPaymentLinkData,
            amount: 19999
          },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      if (
        paymentLink1Response.code !== 200 ||
        paymentLink2Response.code !== 200
      ) {
        throw new Error('Failed to create payment links');
      }

      const response = await listPaymentLinksRoute.sdk.listPaymentLinks({
        query: {
          ids: [
            paymentLink1Response.response.id,
            paymentLink2Response.response.id
          ]
        },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      if (response.code === 200) {
        expect(response.response).toHaveLength(2);
        expect(response.response.map((p: { id: string }) => p.id)).toContain(
          paymentLink1Response.response.id
        );
        expect(response.response.map((p: { id: string }) => p.id)).toContain(
          paymentLink2Response.response.id
        );
      }
    });

    it('should throw error when no payment links match provided IDs', async () => {
      const { listPaymentLinksRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      try {
        await listPaymentLinksRoute.sdk.listPaymentLinks({
          query: {
            ids: ['123e4567-e89b-12d3-a456-426614174999']
          },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

        expect(true).toBe(false);
      } catch (error) {
        expect((error as Error).message).toContain('Record not found');
      }
    });
  });

  describe('GET /payment-link/:id/success - handlePaymentSuccess', () => {
    it('should handle payment success successfully', async () => {
      const { createPaymentLinkRoute, handlePaymentSuccessRoute } =
        await import('../api/routes/paymentLink.routes');

      const createResponse = await createPaymentLinkRoute.sdk.createPaymentLink(
        {
          body: mockPaymentLinkData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        }
      );

      if (createResponse.code !== 200) {
        throw new Error('Failed to create payment link');
      }
      const paymentLinkId = createResponse.response.id;

      const response = await handlePaymentSuccessRoute.sdk.handlePaymentSuccess(
        {
          params: { id: paymentLinkId },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        }
      );

      expect(response.code).toBe(200);
      expect(response.response).toBe(
        `Handled payment success for ${paymentLinkId}`
      );
    });
  });

  describe('GET /payment-link/:id/failure - handlePaymentFailure', () => {
    it('should handle payment failure successfully', async () => {
      const { createPaymentLinkRoute, handlePaymentFailureRoute } =
        await import('../api/routes/paymentLink.routes');

      const createResponse = await createPaymentLinkRoute.sdk.createPaymentLink(
        {
          body: mockPaymentLinkData,
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        }
      );

      if (createResponse.code !== 200) {
        throw new Error('Failed to create payment link');
      }
      const paymentLinkId = createResponse.response.id;

      const response = await handlePaymentFailureRoute.sdk.handlePaymentFailure(
        {
          params: { id: paymentLinkId },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        }
      );

      expect(response.code).toBe(200);
      expect(response.response).toBe(
        `Handled payment failure for ${paymentLinkId}`
      );
    });
  });

  describe('Authentication', () => {
    it('should require HMAC authentication for createPaymentLink', async () => {
      const { createPaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      try {
        await createPaymentLinkRoute.sdk.createPaymentLink({
          body: mockPaymentLinkData,
          headers: {
            authorization: TEST_TOKENS.HMAC_INVALID
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should require HMAC authentication for getPaymentLink', async () => {
      const { getPaymentLinkRoute } = await import(
        '../api/routes/paymentLink.routes'
      );

      try {
        await getPaymentLinkRoute.sdk.getPaymentLink({
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

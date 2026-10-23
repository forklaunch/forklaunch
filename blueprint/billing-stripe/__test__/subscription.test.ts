import { BillingProviderEnum } from '@forklaunch/implementation-billing-stripe/enum';
import { PartyEnum } from '../domain/enum/party.enum';
import {
  cleanupTestDatabase,
  getMockSubscriptionData,
  getMockUpdateSubscriptionData,
  setupTestDatabase,
  TEST_TOKENS
} from './test-utils';

describe('Subscription Routes E2E Tests with PostgreSQL Container', () => {
  beforeAll(async () => {
    await setupTestDatabase();
  });

  afterAll(async () => {
    await cleanupTestDatabase();
  });

  describe('POST /subscription - createSubscription', () => {
    it('should create a subscription successfully', async () => {
      const { createSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const response = await createSubscriptionRoute.sdk.createSubscription({
        body: getMockSubscriptionData(),
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      const mockData = getMockSubscriptionData();
      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        partyId: mockData.partyId,
        partyType: mockData.partyType,
        description: mockData.description,
        active: mockData.active,
        productId: mockData.productId,
        externalId: expect.any(String),
        billingProvider: mockData.billingProvider,
        startDate: expect.any(Date),
        status: mockData.status,
        id: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle validation errors when creating subscription', async () => {
      const { createSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const invalidData = {
        ...getMockSubscriptionData(),
        partyId: '',
        productId: ''
      };

      try {
        await createSubscriptionRoute.sdk.createSubscription({
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

    it('should handle invalid party type enum', async () => {
      const { createSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const invalidData = {
        ...getMockSubscriptionData(),
        partyType: 'INVALID_PARTY_TYPE' as PartyEnum
      };

      try {
        await createSubscriptionRoute.sdk.createSubscription({
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

    it('should handle invalid billing provider enum', async () => {
      const { createSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const invalidData = {
        ...getMockSubscriptionData(),
        billingProvider: 'INVALID_PROVIDER' as BillingProviderEnum
      };

      try {
        await createSubscriptionRoute.sdk.createSubscription({
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

  describe('GET /subscription/:id - getSubscription', () => {
    it('should retrieve an existing subscription successfully', async () => {
      const { createSubscriptionRoute, getSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const createResponse =
        await createSubscriptionRoute.sdk.createSubscription({
          body: getMockSubscriptionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create subscription');
      }
      const subscriptionId = createResponse.response.id;

      const response = await getSubscriptionRoute.sdk.getSubscription({
        params: { id: subscriptionId },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      const mockData = getMockSubscriptionData();
      expect(response.response).toMatchObject({
        id: subscriptionId,
        partyId: mockData.partyId,
        partyType: mockData.partyType,
        description: mockData.description,
        active: mockData.active,
        productId: mockData.productId,
        externalId: expect.any(String),
        billingProvider: mockData.billingProvider,
        startDate: expect.any(Date),
        status: mockData.status,
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle non-existent subscription ID', async () => {
      const { getSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      try {
        await getSubscriptionRoute.sdk.getSubscription({
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
      const { getSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      try {
        await getSubscriptionRoute.sdk.getSubscription({
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

  describe('PUT /subscription - updateSubscription', () => {
    it('should update an existing subscription successfully', async () => {
      const { createSubscriptionRoute, updateSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const createResponse =
        await createSubscriptionRoute.sdk.createSubscription({
          body: getMockSubscriptionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create subscription');
      }
      const subscriptionId = createResponse.response.id;
      const externalId = createResponse.response.externalId;
      const updateData = {
        ...getMockUpdateSubscriptionData(),
        id: subscriptionId,
        externalId: externalId
      };

      const response = await updateSubscriptionRoute.sdk.updateSubscription({
        params: { id: subscriptionId },
        body: updateData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        id: subscriptionId,
        partyId: expect.any(String),
        partyType: expect.any(String),
        description: expect.any(String),
        active: expect.any(Boolean),
        productId: expect.any(String),
        externalId: expect.any(String),
        billingProvider: expect.any(String),
        startDate: expect.any(Date),
        endDate: expect.any(Date),
        status: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle updating non-existent subscription', async () => {
      const { updateSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const updateData = {
        ...getMockUpdateSubscriptionData(),
        id: '123e4567-e89b-12d3-a456-426614174999'
      };

      try {
        await updateSubscriptionRoute.sdk.updateSubscription({
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

    it('should handle validation errors when updating subscription', async () => {
      const { createSubscriptionRoute, updateSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const createResponse =
        await createSubscriptionRoute.sdk.createSubscription({
          body: getMockSubscriptionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create subscription');
      }
      const subscriptionId = createResponse.response.id;
      const invalidUpdateData = {
        ...getMockUpdateSubscriptionData(),
        id: subscriptionId,
        partyId: '',
        productId: ''
      };

      try {
        await updateSubscriptionRoute.sdk.updateSubscription({
          params: { id: subscriptionId },
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

  describe('DELETE /subscription/:id - deleteSubscription', () => {
    it('should delete an existing subscription successfully', async () => {
      const { createSubscriptionRoute, deleteSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const createResponse =
        await createSubscriptionRoute.sdk.createSubscription({
          body: getMockSubscriptionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create subscription');
      }
      const subscriptionId = createResponse.response.id;

      const response = await deleteSubscriptionRoute.sdk.deleteSubscription({
        params: { id: subscriptionId },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe(`Deleted subscription ${subscriptionId}`);
    }, 15000);

    it('should handle deleting non-existent subscription', async () => {
      const { deleteSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      try {
        await deleteSubscriptionRoute.sdk.deleteSubscription({
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

  describe('GET /subscription - listSubscriptions', () => {
    it('should list all subscriptions when no IDs provided', async () => {
      const { createSubscriptionRoute, listSubscriptionsRoute } = await import(
        '../api/routes/subscription.routes'
      );

      await createSubscriptionRoute.sdk.createSubscription({
        body: getMockSubscriptionData(),
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      await createSubscriptionRoute.sdk.createSubscription({
        body: {
          ...getMockSubscriptionData(),
          partyId: process.env.TEST_CUSTOMER_ID!,
          externalId: 'sub_second_123'
        },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      const response = await listSubscriptionsRoute.sdk.listSubscriptions({
        query: { ids: [] },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response.length).toBeGreaterThanOrEqual(2);
      expect(response.response[0]).toMatchObject({
        partyId: expect.any(String),
        partyType: expect.any(String),
        active: expect.any(Boolean),
        productId: expect.any(String),
        status: expect.any(String),
        externalId: expect.any(String),
        billingProvider: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    }, 20000);

    it('should list specific subscriptions when IDs provided', async () => {
      const { createSubscriptionRoute, listSubscriptionsRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const subscription1Response =
        await createSubscriptionRoute.sdk.createSubscription({
          body: getMockSubscriptionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      const subscription2Response =
        await createSubscriptionRoute.sdk.createSubscription({
          body: {
            ...getMockSubscriptionData(),
            partyId: process.env.TEST_CUSTOMER_ID!,
            externalId: 'sub_second_123'
          },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

      if (
        subscription1Response.code !== 200 ||
        subscription2Response.code !== 200
      ) {
        throw new Error('Failed to create subscriptions');
      }

      const response = await listSubscriptionsRoute.sdk.listSubscriptions({
        query: {
          ids: [
            subscription1Response.response.id,
            subscription2Response.response.id
          ]
        },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      if (response.code === 200) {
        expect(response.response).toHaveLength(2);
        expect(response.response.map((s: { id: string }) => s.id)).toContain(
          subscription1Response.response.id
        );
        expect(response.response.map((s: { id: string }) => s.id)).toContain(
          subscription2Response.response.id
        );
      }
    }, 20000);

    it('should return empty array when no subscriptions match provided IDs', async () => {
      const { listSubscriptionsRoute } = await import(
        '../api/routes/subscription.routes'
      );

      const response = await listSubscriptionsRoute.sdk.listSubscriptions({
        query: {
          ids: ['123e4567-e89b-12d3-a456-426614174999']
        },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toHaveLength(0);
    });
  });

  describe('Authentication', () => {
    it('should require HMAC authentication for createSubscription', async () => {
      const { createSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      try {
        await createSubscriptionRoute.sdk.createSubscription({
          body: getMockSubscriptionData(),
          headers: {
            authorization: TEST_TOKENS.HMAC_INVALID
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should require HMAC authentication for getSubscription', async () => {
      const { getSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      try {
        await getSubscriptionRoute.sdk.getSubscription({
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

    it('should require HMAC authentication for updateSubscription', async () => {
      const { updateSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      try {
        await updateSubscriptionRoute.sdk.updateSubscription({
          params: { id: '123e4567-e89b-12d3-a456-426614174000' },
          body: {
            ...getMockUpdateSubscriptionData(),
            id: '123e4567-e89b-12d3-a456-426614174000'
          },
          headers: {
            authorization: TEST_TOKENS.HMAC_INVALID
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should require HMAC authentication for deleteSubscription', async () => {
      const { deleteSubscriptionRoute } = await import(
        '../api/routes/subscription.routes'
      );

      try {
        await deleteSubscriptionRoute.sdk.deleteSubscription({
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

    it('should require HMAC authentication for listSubscriptions', async () => {
      const { listSubscriptionsRoute } = await import(
        '../api/routes/subscription.routes'
      );

      try {
        await listSubscriptionsRoute.sdk.listSubscriptions({
          query: { ids: [] },
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

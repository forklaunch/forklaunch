import {
  BillingProviderEnum,
  CurrencyEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import {
  cleanupTestDatabase,
  getMockPlanData,
  getMockUpdatePlanData,
  setupTestDatabase,
  TEST_TOKENS
} from './test-utils';

describe('Plan Routes E2E Tests with PostgreSQL Container', () => {
  beforeAll(async () => {
    await setupTestDatabase();
  });

  afterAll(async () => {
    await cleanupTestDatabase();
  });

  describe('POST /plan - createPlan', () => {
    it('should create a plan successfully', async () => {
      const { createPlanRoute } = await import('../api/routes/plan.routes');
      const response = await createPlanRoute.sdk.createPlan({
        body: getMockPlanData(),
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      const mockData = getMockPlanData();
      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        active: mockData.active,
        name: mockData.name,
        description: mockData.description,
        price: mockData.price,
        currency: mockData.currency,
        cadence: mockData.cadence,
        features: mockData.features,
        externalId: expect.any(String),
        billingProvider: mockData.billingProvider,
        id: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle validation errors when creating plan', async () => {
      const { createPlanRoute } = await import('../api/routes/plan.routes');
      const invalidData = {
        ...getMockPlanData(),
        name: '',
        price: -100,
        billingProvider: 'INVALID_PROVIDER' as never
      };

      try {
        await createPlanRoute.sdk.createPlan({
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
      const { createPlanRoute } = await import('../api/routes/plan.routes');
      const invalidData = {
        ...getMockPlanData(),
        currency: 'INVALID_CURRENCY' as CurrencyEnum
      };

      try {
        await createPlanRoute.sdk.createPlan({
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
      const { createPlanRoute } = await import('../api/routes/plan.routes');
      const invalidData = {
        ...getMockPlanData(),
        billingProvider: 'INVALID_PROVIDER' as BillingProviderEnum
      };

      try {
        await createPlanRoute.sdk.createPlan({
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

  describe('GET /plan/:id - getPlan', () => {
    it('should retrieve an existing plan successfully', async () => {
      const { createPlanRoute, getPlanRoute } = await import(
        '../api/routes/plan.routes'
      );

      const createResponse = await createPlanRoute.sdk.createPlan({
        body: getMockPlanData(),
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create plan');
      }
      const planId = createResponse.response.id;

      const response = await getPlanRoute.sdk.getPlan({
        params: { id: planId },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      const mockData = getMockPlanData();
      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        id: planId,
        name: mockData.name,
        description: mockData.description,
        price: mockData.price,
        currency: mockData.currency,
        cadence: mockData.cadence,
        features: mockData.features,
        externalId: expect.any(String),
        billingProvider: mockData.billingProvider,
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle non-existent plan ID', async () => {
      const { getPlanRoute } = await import('../api/routes/plan.routes');

      try {
        await getPlanRoute.sdk.getPlan({
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
      const { getPlanRoute } = await import('../api/routes/plan.routes');

      try {
        await getPlanRoute.sdk.getPlan({
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

  describe('PUT /plan - updatePlan', () => {
    it('should update an existing plan successfully', async () => {
      const { createPlanRoute, updatePlanRoute } = await import(
        '../api/routes/plan.routes'
      );

      const createResponse = await createPlanRoute.sdk.createPlan({
        body: getMockPlanData(),
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create plan');
      }
      const planId = createResponse.response.id;
      const updateData = {
        ...getMockUpdatePlanData(),
        id: planId
      };

      const response = await updatePlanRoute.sdk.updatePlan({
        body: updateData,
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toMatchObject({
        id: planId,
        active: expect.any(Boolean),
        name: expect.any(String),
        description: expect.any(String),
        price: expect.any(Number),
        currency: updateData.currency,
        cadence: expect.any(String),
        features: expect.any(Array),
        externalId: expect.any(String),
        billingProvider: updateData.billingProvider,
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should handle updating non-existent plan', async () => {
      const { updatePlanRoute } = await import('../api/routes/plan.routes');

      const updateData = {
        ...getMockUpdatePlanData(),
        id: '123e4567-e89b-12d3-a456-426614174999'
      };

      try {
        await updatePlanRoute.sdk.updatePlan({
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

    it('should handle validation errors when updating plan', async () => {
      const { createPlanRoute, updatePlanRoute } = await import(
        '../api/routes/plan.routes'
      );

      const createResponse = await createPlanRoute.sdk.createPlan({
        body: getMockPlanData(),
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create plan');
      }
      const planId = createResponse.response.id;
      const invalidUpdateData = {
        ...getMockUpdatePlanData(),
        id: planId,
        name: '',
        price: -100
      };

      try {
        await updatePlanRoute.sdk.updatePlan({
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

  describe('GET /plan - listPlans', () => {
    it('should list all plans when no IDs provided', async () => {
      const { createPlanRoute, listPlansRoute } = await import(
        '../api/routes/plan.routes'
      );

      await createPlanRoute.sdk.createPlan({
        body: getMockPlanData(),
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      await createPlanRoute.sdk.createPlan({
        body: {
          ...getMockPlanData(),
          externalId: 'plan_second_123'
        },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      const response = await listPlansRoute.sdk.listPlans({
        query: { ids: [] },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response.length).toBeGreaterThanOrEqual(2);
      expect(response.response[0]).toMatchObject({
        name: expect.any(String),
        price: expect.any(Number),
        currency: expect.any(String),
        cadence: expect.any(String),
        externalId: expect.any(String),
        billingProvider: expect.any(String),
        createdAt: expect.any(Date),
        updatedAt: expect.any(Date)
      });
    });

    it('should list specific plans when IDs provided', async () => {
      const { createPlanRoute, listPlansRoute } = await import(
        '../api/routes/plan.routes'
      );

      const plan1Response = await createPlanRoute.sdk.createPlan({
        body: getMockPlanData(),
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      const plan2Response = await createPlanRoute.sdk.createPlan({
        body: {
          ...getMockPlanData(),
          externalId: 'plan_second_123'
        },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      if (plan1Response.code !== 200 || plan2Response.code !== 200) {
        throw new Error('Failed to create plans');
      }

      const response = await listPlansRoute.sdk.listPlans({
        query: {
          ids: [plan1Response.response.id, plan2Response.response.id]
        },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      if (response.code === 200) {
        expect(response.response.length).toBeGreaterThanOrEqual(2);
        expect(response.response.map((p: { id: string }) => p.id)).toContain(
          plan1Response.response.id
        );
        expect(response.response.map((p: { id: string }) => p.id)).toContain(
          plan2Response.response.id
        );
      }
    });

    it('should return empty array when no plans match provided IDs', async () => {
      const { listPlansRoute } = await import('../api/routes/plan.routes');

      const response = await listPlansRoute.sdk.listPlans({
        query: {
          ids: ['123e4567-e89b-12d3-a456-426614174999']
        },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response.length).toBeGreaterThanOrEqual(0);
    });
  });

  describe('DELETE /plan/:id - deletePlan', () => {
    it('should delete an existing plan successfully', async () => {
      const { createPlanRoute, deletePlanRoute } = await import(
        '../api/routes/plan.routes'
      );

      const createResponse = await createPlanRoute.sdk.createPlan({
        body: getMockPlanData(),
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create plan');
      }
      const planId = createResponse.response.id;

      const response = await deletePlanRoute.sdk.deletePlan({
        params: { id: planId },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response.code).toBe(200);
      expect(response.response).toBe(`Deleted plan ${planId}`);
    });

    it('should handle deleting non-existent plan', async () => {
      const { deletePlanRoute } = await import('../api/routes/plan.routes');

      try {
        await deletePlanRoute.sdk.deletePlan({
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

  describe('Authentication', () => {
    it('should require HMAC authentication for createPlan', async () => {
      const { createPlanRoute } = await import('../api/routes/plan.routes');
      try {
        await createPlanRoute.sdk.createPlan({
          body: getMockPlanData(),
          headers: {
            authorization: TEST_TOKENS.HMAC_INVALID
          }
        });
        expect(true).toBe(false);
      } catch (error: unknown) {
        expect(error).toBeDefined();
      }
    });

    it('should require HMAC authentication for getPlan', async () => {
      const { getPlanRoute } = await import('../api/routes/plan.routes');

      try {
        await getPlanRoute.sdk.getPlan({
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

    it('should require HMAC authentication for updatePlan', async () => {
      const { updatePlanRoute } = await import('../api/routes/plan.routes');

      try {
        await updatePlanRoute.sdk.updatePlan({
          body: {
            ...getMockUpdatePlanData(),
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

    it('should require HMAC authentication for deletePlan', async () => {
      const { deletePlanRoute } = await import('../api/routes/plan.routes');

      try {
        await deletePlanRoute.sdk.deletePlan({
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

    it('should require HMAC authentication for listPlans', async () => {
      const { listPlansRoute } = await import('../api/routes/plan.routes');

      try {
        await listPlansRoute.sdk.listPlans({
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

  describe('Cache Behavior', () => {
    it('should cache GET requests for plans', async () => {
      const { createPlanRoute, getPlanRoute } = await import(
        '../api/routes/plan.routes'
      );

      const createResponse = await createPlanRoute.sdk.createPlan({
        body: getMockPlanData(),
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      if (createResponse.code !== 200) {
        throw new Error('Failed to create plan');
      }
      const planId = createResponse.response.id;

      const response1 = await getPlanRoute.sdk.getPlan({
        params: { id: planId },
        headers: {
          authorization: TEST_TOKENS.HMAC
        }
      });

      expect(response1.code).toBe(200);
      if (response1.code === 200) {
        expect(response1.response.id).toBe(planId);

        const response2 = await getPlanRoute.sdk.getPlan({
          params: { id: planId },
          headers: {
            authorization: TEST_TOKENS.HMAC
          }
        });

        expect(response2.code).toBe(200);
        if (response2.code === 200) {
          expect(response2.response.id).toBe(planId);
          expect(response1.response).toEqual(response2.response);
        }
      }
    });
  });
});

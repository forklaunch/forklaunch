#!/usr/bin/env tsx
/**
 * Seed Plans from Stripe
 *
 * Seeds the database Plan table with data from Stripe products/prices.
 * Run this AFTER sync-stripe-plans.ts to ensure database matches Stripe.
 *
 * Usage:
 *   STRIPE_SECRET_KEY=sk_test_... npm run seed:plans
 */

import { BillingPlanEnum } from '@forklaunch/blueprint-core';
import {
  BillingProviderEnum,
  CurrencyEnum,
  PlanCadenceEnum
} from '@forklaunch/implementation-billing-stripe/enum';
import { MikroORM } from '@mikro-orm/core';
import Stripe from 'stripe';
import mikroOrmConfig from '../mikro-orm.config';
import { Plan } from '../persistence/entities/plan.entity';

const PRODUCT_METADATA_KEY = 'forklaunch_plan';

/**
 * Initialize Stripe client
 */
function initStripe(): Stripe {
  const apiKey = process.env.STRIPE_SECRET_KEY;
  if (!apiKey) {
    throw new Error('STRIPE_SECRET_KEY environment variable is required');
  }

  return new Stripe(apiKey, {
    apiVersion: '2026-03-25.dahlia'
  });
}

/**
 * Find Stripe product by plan name
 */
async function findStripeProduct(
  stripe: Stripe,
  planName: string
): Promise<Stripe.Product | null> {
  const products = await stripe.products.search({
    query: `metadata['${PRODUCT_METADATA_KEY}']:'${planName}'`
  });

  return products.data[0] || null;
}

/**
 * Find active price for product
 */
async function findActivePrice(
  stripe: Stripe,
  productId: string
): Promise<Stripe.Price | null> {
  const prices = await stripe.prices.list({
    product: productId,
    active: true,
    limit: 1
  });

  return prices.data[0] || null;
}

/**
 * Convert Stripe interval to PlanCadenceEnum
 */
function stripeToCadence(interval: string): PlanCadenceEnum {
  switch (interval) {
    case 'month':
      return PlanCadenceEnum.MONTHLY;
    case 'year':
      return PlanCadenceEnum.ANNUALLY;
    default:
      return PlanCadenceEnum.MONTHLY;
  }
}

/**
 * Seed plans from Stripe
 */
async function seedPlans() {
  console.log('=================================');
  console.log('Seed Plans from Stripe');
  console.log('=================================\n');

  const stripe = initStripe();
  const orm = await MikroORM.init(mikroOrmConfig);
  const em = orm.em.fork();

  const planNames = Object.values(BillingPlanEnum);
  let seeded = 0;
  let skipped = 0;

  for (const planName of planNames) {
    console.log(`Processing: ${planName}`);

    // Find Stripe product
    const product = await findStripeProduct(stripe, planName as string);
    if (!product) {
      console.log(`  ✗ Stripe product not found for ${planName}`);
      console.log(`    Run 'npm run sync:stripe' first`);
      skipped++;
      continue;
    }

    // Find active price
    const price = await findActivePrice(stripe, product.id);
    if (!price) {
      console.log(`  ✗ No active price found for ${planName}`);
      skipped++;
      continue;
    }

    // Check if plan already exists in database
    const existing = await em.findOne(Plan, {
      externalId: product.id
    });

    if (existing) {
      console.log(`  ⊙ Plan already exists, updating: ${planName}`);

      // Update existing plan
      existing.name = product.name;
      existing.description = product.description || '';
      existing.price = price.unit_amount || 0;
      existing.currency =
        (price.currency.toUpperCase() as CurrencyEnum) || CurrencyEnum.USD;
      existing.cadence = stripeToCadence(price.recurring?.interval || 'month');
      existing.active = product.active;
      existing.features = JSON.parse(product.metadata.features || '[]');
      existing.providerFields = product;
      existing.updatedAt = new Date();

      await em.persist(existing).flush();
      console.log(`  ✓ Updated: ${planName}`);
    } else {
      console.log(`  + Creating new plan: ${planName}`);

      // Create new plan
      const plan = em.create(Plan, {
        externalId: product.id,
        name: product.name,
        description: product.description || '',
        price: price.unit_amount || 0,
        currency:
          (price.currency.toUpperCase() as CurrencyEnum) || CurrencyEnum.USD,
        cadence: stripeToCadence(price.recurring?.interval || 'month'),
        active: product.active,
        features: JSON.parse(product.metadata.features || '[]'),
        providerFields: product,
        billingProvider: BillingProviderEnum.STRIPE
      });

      await em.persist(plan).flush();
      console.log(`  ✓ Created: ${planName}`);
      seeded++;
    }
  }

  await orm.close();

  console.log('\n=================================');
  console.log('Seed Summary');
  console.log('=================================');
  console.log(`Seeded:  ${seeded}`);
  console.log(`Skipped: ${skipped}`);
  console.log(`Total:   ${planNames.length}`);

  if (seeded > 0) {
    console.log('\n✓ Plans seeded successfully!');
  }
}

// Run the seeder
seedPlans().catch((error) => {
  console.error('Fatal error:', error);
  process.exit(1);
});

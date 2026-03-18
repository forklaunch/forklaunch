#!/usr/bin/env tsx
/**
 * Stripe Plan Sync Script
 *
 * Syncs billing plans from constants to Stripe as Products and Prices.
 * This ensures Stripe has the correct products/prices configured based on your plan definitions.
 *
 * Usage:
 *   STRIPE_SECRET_KEY=sk_test_... npm run sync:stripe
 *   STRIPE_SECRET_KEY=sk_test_... DRY_RUN=true npm run sync:stripe  # Preview changes
 */

import {
  BillingPlan,
  BillingPlanEnum,
  PLAN_FEATURES,
  PLAN_LIMITS
} from '@forklaunch/blueprint-core';
import Stripe from 'stripe';

// Plan pricing configuration
// Customize these values for your application
const PLAN_PRICING = {
  [BillingPlanEnum.FREE]: {
    price: 0,
    currency: 'usd',
    interval: 'month' as const,
    description: 'Free tier with essential features'
  },
  [BillingPlanEnum.PRO]: {
    price: 2900, // $29.00
    currency: 'usd',
    interval: 'month' as const,
    description: 'Professional tier with advanced features'
  },
  [BillingPlanEnum.ENTERPRISE]: {
    price: 9900, // $99.00
    currency: 'usd',
    interval: 'month' as const,
    description: 'Enterprise tier with all features and unlimited resources'
  }
} as const;

// Product metadata tags
const PRODUCT_METADATA_KEY = 'forklaunch_plan';

interface SyncResult {
  productId: string;
  priceId: string;
  planName: string;
  action: 'created' | 'updated' | 'unchanged';
}

/**
 * Initialize Stripe client
 */
function initStripe(): Stripe {
  const apiKey = process.env.STRIPE_SECRET_KEY;
  if (!apiKey) {
    throw new Error('STRIPE_SECRET_KEY environment variable is required');
  }

  return new Stripe(apiKey, {
    apiVersion: '2026-02-25.clover'
  });
}

/**
 * Find existing product by plan name
 */
async function findExistingProduct(
  stripe: Stripe,
  planName: string
): Promise<Stripe.Product | null> {
  const products = await stripe.products.search({
    query: `metadata['${PRODUCT_METADATA_KEY}']:'${planName}'`
  });

  return products.data[0] || null;
}

/**
 * Create or update a Stripe product for a plan
 */
async function syncProduct(
  stripe: Stripe,
  planName: string,
  dryRun: boolean
): Promise<{
  product: Stripe.Product;
  action: 'created' | 'updated' | 'unchanged';
}> {
  const config = PLAN_PRICING[planName as BillingPlan];
  const features = PLAN_FEATURES[planName] || [];
  const limits = PLAN_LIMITS[planName] || {};

  const productData: Stripe.ProductCreateParams = {
    name: `${planName.charAt(0).toUpperCase() + planName.slice(1)} Plan`,
    description: config.description,
    metadata: {
      [PRODUCT_METADATA_KEY]: planName,
      features: JSON.stringify(features),
      maxEnvironments: String(limits.maxEnvironments ?? 0),
      maxServices: String(limits.maxServices ?? 0),
      maxWorkers: String(limits.maxWorkers ?? 0),
      maxMonthlyDeployments: String(limits.maxMonthlyDeployments ?? 0)
    }
  };

  const existingProduct = await findExistingProduct(stripe, planName);

  if (existingProduct) {
    // Check if update is needed
    const needsUpdate =
      existingProduct.name !== productData.name ||
      existingProduct.description !== productData.description ||
      JSON.stringify(existingProduct.metadata) !==
      JSON.stringify(productData.metadata);

    if (!needsUpdate) {
      console.log(`  ✓ Product already up to date: ${planName}`);
      return { product: existingProduct, action: 'unchanged' };
    }

    if (dryRun) {
      console.log(`  → Would update product: ${planName}`);
      return { product: existingProduct, action: 'updated' };
    }

    const updated = await stripe.products.update(existingProduct.id, {
      name: productData.name,
      description: productData.description,
      metadata: productData.metadata
    });

    console.log(`  ✓ Updated product: ${planName} (${updated.id})`);
    return { product: updated, action: 'updated' };
  }

  if (dryRun) {
    console.log(`  → Would create product: ${planName}`);
    return {
      product: { id: 'prod_dry_run', name: productData.name } as Stripe.Product,
      action: 'created'
    };
  }

  const created = await stripe.products.create(productData);
  console.log(`  ✓ Created product: ${planName} (${created.id})`);
  return { product: created, action: 'created' };
}

/**
 * Find active price for a product
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
 * Create or update a Stripe price for a product
 */
async function syncPrice(
  stripe: Stripe,
  product: Stripe.Product,
  planName: string,
  dryRun: boolean
): Promise<{
  price: Stripe.Price;
  action: 'created' | 'updated' | 'unchanged';
}> {
  const config = PLAN_PRICING[planName as BillingPlan];

  const priceData: Stripe.PriceCreateParams = {
    product: product.id,
    unit_amount: config.price,
    currency: config.currency,
    recurring: {
      interval: config.interval
    },
    metadata: {
      [PRODUCT_METADATA_KEY]: planName
    }
  };

  const existingPrice = await findActivePrice(stripe, product.id);

  if (existingPrice) {
    // Check if price matches
    const priceMatches =
      existingPrice.unit_amount === priceData.unit_amount &&
      existingPrice.currency === priceData.currency &&
      existingPrice.recurring?.interval === priceData.recurring?.interval;

    if (priceMatches) {
      console.log(`  ✓ Price already up to date: ${planName}`);
      return { price: existingPrice, action: 'unchanged' };
    }

    if (dryRun) {
      console.log(`  → Would create new price (price changed): ${planName}`);
      return { price: existingPrice, action: 'created' };
    }

    // Deactivate old price and create new one
    await stripe.prices.update(existingPrice.id, { active: false });
    const newPrice = await stripe.prices.create(priceData);

    console.log(
      `  ✓ Created new price (deactivated old): ${planName} (${newPrice.id})`
    );
    return { price: newPrice, action: 'created' };
  }

  if (dryRun) {
    console.log(`  → Would create price: ${planName}`);
    return {
      price: {
        id: 'price_dry_run',
        unit_amount: priceData.unit_amount
      } as Stripe.Price,
      action: 'created'
    };
  }

  const created = await stripe.prices.create(priceData);
  console.log(`  ✓ Created price: ${planName} (${created.id})`);
  return { price: created, action: 'created' };
}

/**
 * Sync a single plan to Stripe
 */
async function syncPlan(
  stripe: Stripe,
  planName: string,
  dryRun: boolean
): Promise<SyncResult> {
  console.log(`\nSyncing plan: ${planName}`);

  const { product, action: productAction } = await syncProduct(
    stripe,
    planName,
    dryRun
  );
  const { price, action: priceAction } = await syncPrice(
    stripe,
    product,
    planName,
    dryRun
  );

  const action =
    productAction === 'created' || priceAction === 'created'
      ? 'created'
      : productAction === 'updated' || priceAction === 'updated'
        ? 'updated'
        : 'unchanged';

  return {
    productId: product.id,
    priceId: price.id,
    planName,
    action
  };
}

/**
 * Main sync function
 */
async function main() {
  const dryRun = process.env.DRY_RUN === 'true';

  console.log('=================================');
  console.log('Stripe Plan Sync');
  console.log('=================================');

  if (dryRun) {
    console.log('🔍 DRY RUN MODE - No changes will be made\n');
  }

  const stripe = initStripe();

  // Get all plan names from the enum
  const planNames = Object.values(BillingPlanEnum);

  console.log(`Found ${planNames.length} plans to sync:`);
  planNames.forEach((name) => console.log(`  - ${name}`));

  // Sync each plan
  const results: SyncResult[] = [];
  for (const planName of planNames) {
    try {
      const result = await syncPlan(stripe, planName as BillingPlan, dryRun);
      results.push(result);
    } catch (error) {
      console.error(`  ✗ Error syncing ${planName}:`, error);
      process.exit(1);
    }
  }

  // Print summary
  console.log('\n=================================');
  console.log('Sync Summary');
  console.log('=================================');

  const created = results.filter((r) => r.action === 'created').length;
  const updated = results.filter((r) => r.action === 'updated').length;
  const unchanged = results.filter((r) => r.action === 'unchanged').length;

  console.log(`Created:   ${created}`);
  console.log(`Updated:   ${updated}`);
  console.log(`Unchanged: ${unchanged}`);
  console.log(`Total:     ${results.length}`);

  if (!dryRun && (created > 0 || updated > 0)) {
    console.log('\n✓ Stripe plans synced successfully!');
    console.log('\nProduct & Price IDs:');
    results.forEach((r) => {
      console.log(`  ${r.planName}:`);
      console.log(`    Product: ${r.productId}`);
      console.log(`    Price:   ${r.priceId}`);
    });
  }

  if (dryRun) {
    console.log('\n💡 Run without DRY_RUN=true to apply changes');
  }
}

// Run the script
main().catch((error) => {
  console.error('Fatal error:', error);
  process.exit(1);
});

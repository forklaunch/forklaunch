#!/usr/bin/env tsx
/**
 * Complete Billing Setup
 *
 * Runs both Stripe sync and database seeding in sequence.
 * This is the easiest way to get your billing system configured.
 *
 * Usage:
 *   STRIPE_SECRET_KEY=sk_test_... npm run setup:billing
 *   STRIPE_SECRET_KEY=sk_test_... DRY_RUN=true npm run setup:billing  # Preview
 */

import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const dryRun = process.env.DRY_RUN === 'true';

/**
 * Run a script and wait for completion
 */
function runScript(
  scriptPath: string,
  env: NodeJS.ProcessEnv
): Promise<number> {
  return new Promise((resolve, reject) => {
    const child = spawn('tsx', [scriptPath], {
      stdio: 'inherit',
      env: { ...process.env, ...env }
    });

    child.on('close', (code) => {
      if (code === 0) {
        resolve(code);
      } else {
        reject(new Error(`Script exited with code ${code}`));
      }
    });

    child.on('error', reject);
  });
}

/**
 * Main setup function
 */
async function main() {
  console.log('╔═══════════════════════════════════╗');
  console.log('║   Complete Billing Setup          ║');
  console.log('╚═══════════════════════════════════╝\n');

  if (dryRun) {
    console.log('🔍 DRY RUN MODE - No changes will be made\n');
  }

  if (!process.env.STRIPE_SECRET_KEY) {
    console.error(
      '❌ Error: STRIPE_SECRET_KEY environment variable is required'
    );
    console.error('\nUsage:');
    console.error('  STRIPE_SECRET_KEY=sk_test_... npm run setup:billing');
    process.exit(1);
  }

  try {
    // Step 1: Sync to Stripe
    console.log('Step 1/2: Syncing plans to Stripe...\n');
    await runScript(join(__dirname, 'sync-stripe-plans.ts'), {
      DRY_RUN: dryRun ? 'true' : 'false'
    });

    if (dryRun) {
      console.log('\n✓ Dry run complete!');
      console.log('Run without DRY_RUN=true to apply changes');
      return;
    }

    // Step 2: Seed database
    console.log('\nStep 2/2: Seeding database...\n');
    await runScript(join(__dirname, 'seed-plans-from-stripe.ts'), {});

    console.log('\n╔═══════════════════════════════════╗');
    console.log('║   ✓ Billing Setup Complete!      ║');
    console.log('╚═══════════════════════════════════╝');

    console.log('\nNext steps:');
    console.log('  1. Configure Stripe webhooks');
    console.log('  2. Implement checkout flow');
    console.log('  3. Test subscription creation');
    console.log('\nSee scripts/README.md for more details.');
  } catch (error) {
    console.error('\n❌ Setup failed:', error);
    process.exit(1);
  }
}

// Run setup
void main();

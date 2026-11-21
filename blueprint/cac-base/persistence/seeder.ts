import { withEncryptionContext } from '@forklaunch/core/persistence';
import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { validConfigInjector } from '../mikro-orm.config';
import * as seeders from './seeders';

/**
 * Seeded rows still need a tenant.
 *
 * Columns marked `.compliance('phi'|'pii')` (patient, insurance, ...) are
 * encrypted under the tenant bound when they are written;
 * @forklaunch/core refuses to write one with no tenant bound (it throws
 * `UnboundTenantError`). Seed data belongs to no organization, so it gets an
 * explicit constant. `''` is rejected outright. If you seed for a real
 * organization, bind that organization's id here instead.
 *
 * Nothing is seeded by default (see ./seeders/index.ts); code sets come from
 * the ETL. Seeders only ever run in development.
 */
const SEED_TENANT_ID = '_seed';

export class DatabaseSeeder extends Seeder {
  run(em: EntityManager): Promise<void> {
    if (validConfigInjector.resolve('NODE_ENV') === 'development') {
      return withEncryptionContext(SEED_TENANT_ID, () =>
        this.call(em, Object.values(seeders))
      );
    }
    return Promise.resolve();
  }
}

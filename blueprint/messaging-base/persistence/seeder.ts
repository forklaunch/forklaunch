import { withEncryptionContext } from '@forklaunch/core/persistence';
import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { validConfigInjector } from '../mikro-orm.config';
import * as seeders from './seeders';

/**
 * Seeded rows still need a tenant.
 *
 * Any column marked `.compliance('pii'|'pci'|'phi')` is encrypted under the
 * tenant bound when it is written, and @forklaunch/core 2.x refuses to write
 * one with no tenant bound — it throws `UnboundTenantError` rather than
 * silently using the empty key, which is what used to happen and is why rows
 * became unreadable later under their real owner.
 *
 * Seed data belongs to no organization, so it gets an explicit constant. `''`
 * is NOT an option: it is rejected outright. If you seed for a real
 * organization, bind that organization's id here instead.
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

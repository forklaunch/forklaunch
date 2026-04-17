import { EntityManager } from "@mikro-orm/core";
import { Seeder } from "@mikro-orm/seeder";
import { setupTenantFilter } from "@forklaunch/core/persistence";
import { validConfigInjector } from "../mikro-orm.config";
import * as seeders from "./seeders";

// This is the entry point for all seeders.
// MikroORM's seeder config (in mikro-orm.config.ts) points to this file via glob: 'seeder.ts'.
// To add new seeders:
// 1. Create a new seeder class in ./seeders/ directory
// 2. Export it from ./seeders/index.ts
// 3. It will be automatically picked up by Object.values(seeders) below
export class DatabaseSeeder extends Seeder {
  run(em: EntityManager): Promise<void> {
    // Register tenant filter so setFilterParams works in child seeders
    setupTenantFilter({ em });

    if (validConfigInjector.resolve("NODE_ENV") === "development") {
      return this.call(em, Object.values(seeders));
    }
    return Promise.resolve();
  }
}

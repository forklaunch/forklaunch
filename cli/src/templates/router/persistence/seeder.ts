import { EntityManager } from "@mikro-orm/core";
import { Seeder } from "@mikro-orm/seeder";
import { validConfigInjector } from "../mikro-orm.config";
import * as seeders from "./seeders";

export class DatabaseSeeder extends Seeder {
  run(em: EntityManager): Promise<void> {
    if (validConfigInjector.resolve("NODE_ENV") === "development") {
      return this.call(em, Object.values(seeders));
    }
    return Promise.resolve();
  }
}

import { withAuthEncryptionPolicy } from '../domain/utils/authEncryptionPolicy.util';
import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { validConfigInjector } from '../mikro-orm.config';
import {
  AccountSeeder,
  SessionSeeder,
  UserSeeder,
  VerificationSeeder
} from './seeders';

/** Development identity fixtures follow the same versioned IAM policy as signup. */
export class DatabaseSeeder extends Seeder {
  run(em: EntityManager): Promise<void> {
    if (validConfigInjector.resolve('NODE_ENV') === 'development') {
      // Explicit order: account and session rows reference user.id, and a
      // namespace-object Object.values() iterates alphabetically — which put
      // AccountSeeder before UserSeeder and violated account_user_id_foreign.
      return withAuthEncryptionPolicy(() =>
        this.call(em, [
          UserSeeder,
          AccountSeeder,
          SessionSeeder,
          VerificationSeeder
        ])
      );
    }
    return Promise.resolve();
  }
}

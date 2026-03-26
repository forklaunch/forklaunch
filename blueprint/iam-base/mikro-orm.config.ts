import { number, schemaValidator, string } from '@forklaunch/blueprint-core';

import {
  ComplianceEventSubscriber,
  FieldEncryptor
} from '@forklaunch/core/persistence';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { Migrator } from '@mikro-orm/migrations';
import { defineConfig, Platform, TextType, Type } from '@mikro-orm/postgresql';
import dotenv from 'dotenv';
import * as entities from './persistence/entities';

dotenv.config({ path: getEnvVar('DOTENV_FILE_PATH') });

const configInjector = createConfigInjector(schemaValidator, {
  DB_NAME: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('DB_NAME')
  },
  DB_HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('DB_HOST')
  },
  DB_USER: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('DB_USER')
  },
  DB_PASSWORD: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('DB_PASSWORD')
  },
  DB_PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('DB_PORT'))
  },
  NODE_ENV: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('NODE_ENV')
  },
  ENCRYPTION_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('ENCRYPTION_KEY')
  }
});

export const validConfigInjector = configInjector.validateConfigSingletons(
  getEnvVar('DOTENV_FILE_PATH') ?? '.env'
);
const tokens = validConfigInjector.tokens();

const mikroOrmOptionsConfig = defineConfig({
  dbName: validConfigInjector.resolve(tokens.DB_NAME),
  host: validConfigInjector.resolve(tokens.DB_HOST),
  user: validConfigInjector.resolve(tokens.DB_USER),
  password: validConfigInjector.resolve(tokens.DB_PASSWORD),
  port: validConfigInjector.resolve(tokens.DB_PORT),
  entities: Object.values(entities),
  subscribers: [
    new ComplianceEventSubscriber(
      new FieldEncryptor(validConfigInjector.resolve(tokens.ENCRYPTION_KEY))
    )
  ],
  debug: true,
  extensions: [Migrator],
  migrations: {
    path: './migrations',
    glob: '!(*.d).{js,ts}'
  },
  discovery: {
    getMappedType(type: string, platform: Platform) {
      switch (type.toLowerCase()) {
        case 'string':
        case 'text':
          return Type.getType(TextType);
        default:
          return platform.getDefaultMappedType(type);
      }
    }
  },
  seeder: {
    path: 'dist/persistence',
    glob: 'seeder.js'
  }
});

export default mikroOrmOptionsConfig;

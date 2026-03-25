import { createConfigInjector, getEnvVar, Lifetime } from '@forklaunch/core/services';
import { ComplianceEventSubscriber } from '@forklaunch/core/persistence';
import { FieldEncryptor } from '@forklaunch/core/encryption';
import { Migrator } from '@mikro-orm/migrations{{#is_mongo}}-mongodb{{/is_mongo}}';
import { number, SchemaValidator, string } from '@{{app_name}}/core';
{{^is_mongo}}import { Platform, TextType, Type } from '@mikro-orm/core';{{/is_mongo}}
import { defineConfig } from '@mikro-orm/{{database}}';
import dotenv from 'dotenv';
import * as entities from './persistence/entities';

//! Load the environment variables
dotenv.config({ path: getEnvVar('DOTENV_FILE_PATH') });

//! Create the config injector
const configInjector = createConfigInjector(
  SchemaValidator(),
  {
    DB_NAME: {
      lifetime: Lifetime.Singleton,
      type: string,
      value: getEnvVar('DB_NAME')
    },{{^is_in_memory_database}}
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
    }, {{/is_in_memory_database}}
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
  }
);

//! Validate the config injector
export const validConfigInjector = configInjector.validateConfigSingletons(
  getEnvVar('DOTENV_FILE_PATH')
);
const tokens = validConfigInjector.tokens();

//! Define the mikro-orm options config
const mikroOrmOptionsConfig = defineConfig({ {{#is_mongo}}
  clientUrl: `mongodb://${validConfigInjector.resolve(
    tokens.DB_USER
  )}:${validConfigInjector.resolve(
    tokens.DB_PASSWORD
  )}@${validConfigInjector.resolve(
    tokens.DB_HOST
  )}:${validConfigInjector.resolve(
    tokens.DB_PORT
  )}/${validConfigInjector.resolve(
    tokens.DB_NAME
  )}?authSource=admin&directConnection=true&replicaSet=rs0`,{{/is_mongo}}{{^is_mongo}}
  dbName: validConfigInjector.resolve(
    tokens.DB_NAME
  ),{{^is_in_memory_database}}
  host: validConfigInjector.resolve(
    tokens.DB_HOST
  ),
  user: validConfigInjector.resolve(
    tokens.DB_USER
  ),
  password: validConfigInjector.resolve(
    tokens.DB_PASSWORD
  ),
  port: validConfigInjector.resolve(
    tokens.DB_PORT
  ),{{/is_in_memory_database}}{{/is_mongo}}
  entities: Object.values(entities),
  subscribers: [
    new ComplianceEventSubscriber(
      new FieldEncryptor(validConfigInjector.resolve(tokens.ENCRYPTION_KEY))
    )
  ],
  debug: validConfigInjector.resolve(
    tokens.NODE_ENV
  ) === 'development',
  extensions: [Migrator],{{^is_mongo}}
  discovery: {
    getMappedType(type: string, platform: Platform) {
      // override the mapping for string properties only
      if (type === 'string') {
        return Type.getType(TextType);
      }

      return platform.getDefaultMappedType(type);
    }
  },{{/is_mongo}}
  migrations: {
    path: 'dist/migrations-{{database}}',
    pathTs: 'migrations-{{database}}'
  },
  seeder: {
    path: 'dist/persistence',
    glob: 'seeder.js'
  }{{#is_better_auth}},
  allowGlobalContext: true{{/is_better_auth}}
});

//! Export the mikro-orm options config
export default mikroOrmOptionsConfig;


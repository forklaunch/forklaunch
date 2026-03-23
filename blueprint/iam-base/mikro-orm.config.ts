import { number, schemaValidator, string } from '@forklaunch/blueprint-core';
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
  }
});

export const validConfigInjector = configInjector.validateConfigSingletons(
  getEnvVar('DOTENV_FILE_PATH') ?? '.env'
);

const mikroOrmOptionsConfig = defineConfig({
  dbName: validConfigInjector.resolve('DB_NAME'),
  host: validConfigInjector.resolve('DB_HOST'),
  user: validConfigInjector.resolve('DB_USER'),
  password: validConfigInjector.resolve('DB_PASSWORD'),
  port: validConfigInjector.resolve('DB_PORT'),
  entities: Object.values(entities),
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

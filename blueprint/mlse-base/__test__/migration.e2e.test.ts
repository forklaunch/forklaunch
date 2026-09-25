import { MikroORM } from '@mikro-orm/postgresql';
import * as path from 'path';
import { StartedTestContainer } from 'testcontainers';
import {
  PGVECTOR_IMAGE,
  startPgvectorContainer,
  TEST_DB
} from './pgvector-container';

// mikro-orm.config validates its settings from the environment at import
// time, so it is imported only after the container's settings are in place.
const initOrm = async () => {
  const { default: config } = await import('../mikro-orm.config');
  return MikroORM.init({
    ...config,
    discovery: { ...config.discovery },
    debug: false,
    migrations: {
      path: path.join(__dirname, '../migrations'),
      glob: '!(*.d).{js,ts}',
      // no snapshot file: test files run in parallel and would race on it
      snapshot: false
    }
  });
};

let container: StartedTestContainer;
let orm: Awaited<ReturnType<typeof initOrm>>;

beforeAll(async () => {
  container = await startPgvectorContainer();

  process.env.DB_NAME = TEST_DB.database;
  process.env.DB_HOST = container.getHost();
  process.env.DB_USER = TEST_DB.user;
  process.env.DB_PASSWORD = TEST_DB.password;
  process.env.DB_PORT = String(container.getMappedPort(5432));
  process.env.NODE_ENV = 'test';
  process.env.ENCRYPTION_KEY = '0'.repeat(64);

  orm = await initOrm();
  await orm.migrator.up();
}, 180_000);

afterAll(async () => {
  await orm?.close(true);
  await container?.stop();
});

describe(`mlse migrations on ${PGVECTOR_IMAGE}`, () => {
  it('enables the pgvector extension', async () => {
    const rows = await orm.em
      .getConnection()
      .execute<{ extversion: string }[]>(
        `select extversion from pg_extension where extname = 'vector'`
      );
    expect(rows).toHaveLength(1);
    expect(rows[0].extversion).toMatch(/^\d+\.\d+/);
  });

  it('supports vector distance queries', async () => {
    const rows = await orm.em
      .getConnection()
      .execute<{ distance: number }[]>(
        `select '[1,2,3]'::vector <-> '[1,2,4]'::vector as distance`
      );
    expect(Number(rows[0].distance)).toBeCloseTo(1, 10);
  });

  it('creates the source registry with a unique source key', async () => {
    const { Source } = await import('../persistence/entities/source.entity');
    const em = orm.em.fork();

    em.create(Source, {
      sourceKey: 'openfda',
      name: 'openFDA drug labels',
      tier: 'regulatory',
      licenseTerms: 'Public domain (CC0)',
      commercialUse: true,
      liveQuery: true,
      lastRefreshedAt: null
    });
    await em.flush();

    const saved = await orm.em.fork().findOneOrFail(Source, {
      sourceKey: 'openfda'
    });
    expect(saved.commercialUse).toBe(true);

    const duplicate = orm.em.fork();
    duplicate.create(Source, {
      sourceKey: 'openfda',
      name: 'duplicate',
      tier: 'regulatory',
      licenseTerms: 'Public domain (CC0)',
      commercialUse: true,
      liveQuery: true,
      lastRefreshedAt: null
    });
    await expect(duplicate.flush()).rejects.toThrow();
  });
});

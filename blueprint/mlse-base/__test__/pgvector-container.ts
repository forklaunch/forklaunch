import { GenericContainer, StartedTestContainer, Wait } from 'testcontainers';

// Same image the CLI provisions for mlse applications
// (PGVECTOR_POSTGRES_IMAGE in cli/src/core/docker.rs). The shared
// BlueprintTestHarness always starts plain postgres:latest, which has no
// pgvector, so mlse's database tests start their own container until the
// harness accepts an image override.
export const PGVECTOR_IMAGE = 'pgvector/pgvector:pg18-trixie';

export const TEST_DB = {
  user: 'test_user',
  password: 'test_password',
  database: 'test_db'
};

export async function startPgvectorContainer(): Promise<StartedTestContainer> {
  return new GenericContainer(PGVECTOR_IMAGE)
    .withExposedPorts(5432)
    .withEnvironment({
      POSTGRES_USER: TEST_DB.user,
      POSTGRES_PASSWORD: TEST_DB.password,
      POSTGRES_DB: TEST_DB.database
    })
    .withWaitStrategy(
      Wait.forLogMessage('database system is ready to accept connections', 2)
    )
    .start();
}

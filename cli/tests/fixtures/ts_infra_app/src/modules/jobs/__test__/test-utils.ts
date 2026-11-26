import { BlueprintTestHarness, TestSetupResult } from '@forklaunch/testing';

let harness: BlueprintTestHarness;

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  harness = new BlueprintTestHarness({
    needsRedis: true,
    customEnvVars: {
      PORT: '3000',
      OTEL_LEVEL: 'info',
      QUEUE_NAME: 'test-queue'
    }
  });

  return await harness.setup();
};

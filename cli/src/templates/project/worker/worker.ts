import {
  WorkerFailureHandler,
  WorkerProcessFunction
} from '@forklaunch/interfaces-worker/types';
import { getEnvVar } from '@forklaunch/common';
import { InferEntity } from '@mikro-orm/core';
import dotenv from 'dotenv';
import { createDependencyContainer } from './registrations';
import { {{pascal_case_name}}EventRecord } from './persistence/entities/{{camel_case_name}}EventRecord.entity';

const envFilePath = getEnvVar('DOTENV_FILE_PATH');
dotenv.config({ path: envFilePath });
const { ci, tokens } = createDependencyContainer(envFilePath);

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

const processEvents: WorkerProcessFunction<InferEntity<typeof {{pascal_case_name}}EventRecord>> =
  async (events) => {
    const failedEvents = [];

    for (const event of events) {
      try {
        openTelemetryCollector.info(
          `processing message from ${ci.resolve(tokens.QUEUE_NAME)}: ${event.message}`
        );
        event.processed = true;
      } catch (error) {
        failedEvents.push({
          value: event,
          error: error as Error
        });
      }
    }

    return failedEvents;
  };

const processErrors: WorkerFailureHandler<InferEntity<typeof {{pascal_case_name}}EventRecord>> = async (
  events
) => {
  events.forEach((event) => {
    openTelemetryCollector.error(
      event.error,
      'error processing message',
      event.value
    );
  });
};

const workerConsumer = ci.resolve(
  tokens.WorkerConsumer
);
await workerConsumer(processEvents, processErrors).start()

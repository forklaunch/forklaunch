import {
  WorkerFailureHandler,
  WorkerProcessFunction
} from '@forklaunch/interfaces-worker/types';
import { ci, tokens } from './bootstrapper';
import { type SampleWorkerEventRecord } from './persistence/entities/sampleWorkerRecord.entity';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const s3 = ci.resolve(tokens.S3ObjectStore);

const processEvents: (
  name: string
) => WorkerProcessFunction<SampleWorkerEventRecord> =
  (name: string) => async (events) => {
    const failedEvents = [];

    for (const event of events) {
      try {
        openTelemetryCollector.info(
          `processing message from ${name}: ${event.message}`
        );
        s3.putObject({
          key: event.id,
          value: event
        });
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

const processErrors: WorkerFailureHandler<SampleWorkerEventRecord> = async (
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

const queues = [];

const databaseWorkerConsumer = ci.resolve(tokens.SampleWorkerDatabaseConsumer);
queues.push(
  databaseWorkerConsumer(processEvents('database'), processErrors).start()
);

const redisWorkerConsumer = ci.resolve(tokens.SampleWorkerRedisConsumer);
queues.push(redisWorkerConsumer(processEvents('redis'), processErrors).start());

const bullMqWorkerConsumer = ci.resolve(tokens.SampleWorkerBullMqConsumer);
queues.push(
  bullMqWorkerConsumer(processEvents('bullmq'), processErrors).start()
);

const kafkaWorkerConsumer = ci.resolve(tokens.SampleWorkerKafkaConsumer);
queues.push(kafkaWorkerConsumer(processEvents('kafka'), processErrors).start());

await Promise.all(queues);

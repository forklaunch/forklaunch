import { WorkerEventEntity } from '@forklaunch/interfaces-worker/types';
import { Kafka } from 'kafkajs';
import { KafkaWorkerOptions } from '../domain/types/kafkaWorker.types';
export class KafkaWorkerProducer<
  EventEntity extends WorkerEventEntity,
  Options extends KafkaWorkerOptions
> {
  private producer;
  private connection?: Promise<void>;
  private readonly queueName: string;
  private readonly options: Options;

  constructor(queueName: string, options: Options) {
    this.queueName = queueName;
    this.options = options;
    const kafka = new Kafka({
      clientId: this.options.clientId,
      brokers: this.options.brokers
    });
    this.producer = kafka.producer();
    // Connect eagerly, but make every send wait for it: a send issued while
    // the connection is still being established fails with "The producer is
    // disconnected". A failed attempt is retried by the next send.
    this.connect().catch(() => undefined);
  }

  private connect(): Promise<void> {
    this.connection ??= this.producer.connect().catch((error: unknown) => {
      this.connection = undefined;
      throw error;
    });
    return this.connection;
  }

  async enqueueJob(event: EventEntity): Promise<void> {
    await this.connect();
    await this.producer.send({
      topic: this.queueName,
      messages: [{ value: JSON.stringify([event]) }]
    });
  }

  async enqueueBatchJobs(events: EventEntity[]): Promise<void> {
    await this.connect();
    await this.producer.send({
      topic: this.queueName,
      messages: events.map((event) => ({ value: JSON.stringify(event) }))
    });
  }
}

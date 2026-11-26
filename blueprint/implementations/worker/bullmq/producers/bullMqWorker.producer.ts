import { WorkerProducer } from '@forklaunch/interfaces-worker/interfaces';
import { WorkerEventEntity } from '@forklaunch/interfaces-worker/types';
import { Queue } from 'bullmq';
import { withTenantPrefix } from '../domain/tenantIsolation';
import { BullMqWorkerOptions } from '../domain/types/bullMqWorker.types';

export class BullMqWorkerProducer<
  EventEntity extends WorkerEventEntity,
  Options extends BullMqWorkerOptions
> implements WorkerProducer<EventEntity> {
  private queue;
  private readonly queueName: string;
  private readonly options: Options;

  constructor(queueName: string, options: Options) {
    this.queueName = queueName;
    this.options = options;
    // The tenant namespace is forced on, not defaulted: on a shared pool the
    // ACL only admits `~<appPrefix>:*`, so anything else is denied on every
    // command. See domain/tenantIsolation.ts.
    this.queue = new Queue(
      this.queueName,
      withTenantPrefix({
        ...this.options.queueOptions,
        connection: this.options.queueOptions.connection
      })
    );
  }

  async enqueueJob(event: EventEntity): Promise<void> {
    await this.queue.add(event.id, event, {
      attempts: this.options.retries,
      backoff: {
        type: this.options.backoffType,
        delay: this.options.interval
      }
    });
  }
  async enqueueBatchJobs(events: EventEntity[]): Promise<void> {
    await this.queue.addBulk(
      events.map((event) => ({
        name: event.id,
        data: event,
        opts: {
          attempts: this.options.retries,
          backoff: {
            type: this.options.backoffType,
            delay: this.options.interval
          }
        }
      }))
    );
  }
}

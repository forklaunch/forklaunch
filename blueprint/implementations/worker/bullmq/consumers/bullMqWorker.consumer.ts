import { WorkerConsumer } from '@forklaunch/interfaces-worker/interfaces';
import {
  WorkerEventEntity,
  WorkerFailureHandler,
  WorkerProcessFunction
} from '@forklaunch/interfaces-worker/types';
import { Job, Queue, Worker } from 'bullmq';
import {
  isTenantIsolationError,
  makeIsolationReporter,
  withTenantPrefix
} from '../domain/tenantIsolation';
import { BullMqWorkerOptions } from '../domain/types/bullMqWorker.types';

export class BullMqWorkerConsumer<
  EventEntity extends WorkerEventEntity,
  Options extends BullMqWorkerOptions
> implements WorkerConsumer<EventEntity> {
  private queue: Queue;
  private worker?: Worker;
  protected readonly queueName: string;
  protected readonly options: Options;
  protected readonly processEvents: WorkerProcessFunction<EventEntity>;
  protected readonly failureHandler: WorkerFailureHandler<EventEntity>;
  private readonly reportIsolationFailure = makeIsolationReporter();

  constructor(
    queueName: string,
    options: Options,
    processEvents: WorkerProcessFunction<EventEntity>,
    failureHandler: WorkerFailureHandler<EventEntity>
  ) {
    this.queueName = queueName;
    this.options = options;
    this.processEvents = processEvents;
    this.failureHandler = failureHandler;
    this.queue = new Queue(
      this.queueName,
      withTenantPrefix({
        ...this.options.queueOptions,
        connection: this.options.queueOptions.connection
      })
    );
  }

  async peekEvents(): Promise<EventEntity[]> {
    const jobs = await this.queue.getJobs(['waiting', 'active']);
    return jobs.map((job) => job.data as EventEntity);
  }

  async start(): Promise<void> {
    this.worker = new Worker(
      this.queueName,
      async (job: Job) => {
        const event = job.data as EventEntity;
        await this.processEvents([event]);
      },
      withTenantPrefix(this.options.queueOptions)
    );

    // A NOPERM is the server saying this client may never run that command or
    // touch that key. It cannot become true by trying again, and BullMQ's
    // reconnect has no backoff — left alone this is an unbounded loop that
    // logs a stack trace per attempt. Say it once, then stop consuming so the
    // task is idle and diagnosable instead of expensive and busy.
    this.worker.on('error', (error: Error) => {
      if (!isTenantIsolationError(error)) return;
      this.reportIsolationFailure(this.queueName, error);
      void this.close();
    });

    this.worker.on('failed', (job: Job | undefined, error: Error) => {
      if (job) {
        this.failureHandler([
          {
            value: job.data as EventEntity,
            error
          }
        ]);
      }
    });
  }

  async close(): Promise<void> {
    await this.worker?.close();
    await this.queue.close();
  }
}

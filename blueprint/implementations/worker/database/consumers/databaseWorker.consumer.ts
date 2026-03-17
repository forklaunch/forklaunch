import { WorkerConsumer } from '@forklaunch/interfaces-worker/interfaces';
import {
  WorkerEventEntity,
  WorkerFailureHandler,
  WorkerProcessFunction
} from '@forklaunch/interfaces-worker/types';
import { EntityManager, EntityName } from '@mikro-orm/core';
import { DatabaseWorkerOptions } from '../domain/types/databaseWorker.types';

export class DatabaseWorkerConsumer<
  EventEntity extends WorkerEventEntity,
  Options extends DatabaseWorkerOptions
> implements WorkerConsumer<EventEntity>
{
  protected readonly entityName: EntityName<EventEntity>;
  protected readonly em: EntityManager;
  protected readonly options: Options;
  protected readonly processEvents: WorkerProcessFunction<EventEntity>;
  protected readonly failureHandler: WorkerFailureHandler<EventEntity>;

  constructor(
    entityName: EntityName<EventEntity>,
    em: EntityManager,
    options: Options,
    processEvents: WorkerProcessFunction<EventEntity>,
    failureHandler: WorkerFailureHandler<EventEntity>
  ) {
    this.entityName = entityName;
    this.em = em;
    this.options = options;
    this.processEvents = processEvents;
    this.failureHandler = failureHandler;
  }

  private async retrieveEvents(): Promise<EventEntity[]> {
    return this.em.getRepository(this.entityName).find(
      {
        processed: false,
        retryCount: { $lt: this.options.retries }
      },
      {
        orderBy: { createdAt: 'ASC' }
      }
    );
  }

  private async updateEvents(events: EventEntity[]): Promise<void> {
    await this.em.getRepository(this.entityName).upsertMany(
      events.map((event) => ({
        ...event,
        retryCount: event.retryCount + 1
      }))
    );
  }

  async peekEvents(): Promise<EventEntity[]> {
    return this.retrieveEvents();
  }

  async start(): Promise<void> {
    setInterval(async () => {
      const events = await this.retrieveEvents();
      const failedEvents = await this.processEvents(events);
      await this.failureHandler(failedEvents);
      await this.updateEvents(failedEvents.map((event) => event.value));
      await this.em.flush();
    }, this.options.interval);
  }
}

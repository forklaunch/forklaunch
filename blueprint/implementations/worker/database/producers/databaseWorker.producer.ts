import { WorkerEventEntity } from '@forklaunch/interfaces-worker/types';
import {
  EntityManager,
  EntitySchema,
  RequiredEntityData
} from '@mikro-orm/core';
import { DatabaseWorkerOptions } from '../domain/types/databaseWorker.types';

export class DatabaseWorkerProducer<
  EventEntity extends WorkerEventEntity,
  Options extends DatabaseWorkerOptions
> {
  private readonly em: EntityManager;
  private readonly options: Options;
  private readonly entityName?: EntitySchema<EventEntity>;

  /**
   * @param entityName the event record's entity, the one the matching
   *   `DatabaseWorkerConsumer` reads. MikroORM 7 only persists entity
   *   instances, so without it callers must pass entities (`em.create`)
   *   rather than plain event objects.
   */
  constructor(
    em: EntityManager,
    options: Options,
    entityName?: EntitySchema<EventEntity>
  ) {
    this.em = em;
    this.options = options;
    this.entityName = entityName;
  }

  private toEntity(event: EventEntity): EventEntity {
    // An event is a complete record of its own entity; MikroORM's generic
    // RequiredEntityData cannot see that through the EventEntity parameter.
    return this.entityName
      ? this.em.create(
          this.entityName,
          event as RequiredEntityData<EventEntity>
        )
      : event;
  }

  async enqueueJob(event: EventEntity): Promise<void> {
    await this.em.persist(this.toEntity(event)).flush();
  }

  async enqueueBatchJobs(events: EventEntity[]): Promise<void> {
    await this.em
      .persist(events.map((event) => this.toEntity(event)))
      .flush();
  }
}

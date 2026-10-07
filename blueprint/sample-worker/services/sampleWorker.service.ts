import { BullMqWorkerProducer } from '@forklaunch/implementation-worker-bullmq/producers';
import { BullMqWorkerOptions } from '@forklaunch/implementation-worker-bullmq/types';
import { DatabaseWorkerProducer } from '@forklaunch/implementation-worker-database/producers';
import { DatabaseWorkerOptions } from '@forklaunch/implementation-worker-database/types';
import { KafkaWorkerProducer } from '@forklaunch/implementation-worker-kafka/producers';
import { KafkaWorkerOptions } from '@forklaunch/implementation-worker-kafka/types';
import { RedisWorkerProducer } from '@forklaunch/implementation-worker-redis/producers';
import { RedisWorkerOptions } from '@forklaunch/implementation-worker-redis/types';
import { EntityManager } from '@mikro-orm/core';
import { SampleWorkerService } from '../domain/interfaces/sampleWorkerService.interface';
import {
  SampleWorkerRequestDto,
  SampleWorkerRequestMapper,
  SampleWorkerResponseDto,
  SampleWorkerResponseMapper
} from '../domain/mappers/sampleWorker.mappers';
import { type SampleWorkerEventRecord } from '../persistence/entities';

// BaseSampleWorkerService class that implements the SampleWorkerService interface
export class BaseSampleWorkerService implements SampleWorkerService {
  private em: EntityManager;
  private databaseWorkerProducer: DatabaseWorkerProducer<
    SampleWorkerEventRecord,
    DatabaseWorkerOptions
  >;
  private bullMqWorkerProducer: BullMqWorkerProducer<
    SampleWorkerEventRecord,
    BullMqWorkerOptions
  >;
  private redisWorkerProducer: RedisWorkerProducer<
    SampleWorkerEventRecord,
    RedisWorkerOptions
  >;
  private kafkaWorkerProducer: KafkaWorkerProducer<
    SampleWorkerEventRecord,
    KafkaWorkerOptions
  >;

  constructor(
    em: EntityManager,
    databaseWorkerProducer: DatabaseWorkerProducer<
      SampleWorkerEventRecord,
      DatabaseWorkerOptions
    >,
    bullMqWorkerProducer: BullMqWorkerProducer<
      SampleWorkerEventRecord,
      BullMqWorkerOptions
    >,
    redisWorkerProducer: RedisWorkerProducer<
      SampleWorkerEventRecord,
      RedisWorkerOptions
    >,
    kafkaWorkerProducer: KafkaWorkerProducer<
      SampleWorkerEventRecord,
      KafkaWorkerOptions
    >
  ) {
    this.em = em;
    this.databaseWorkerProducer = databaseWorkerProducer;
    this.bullMqWorkerProducer = bullMqWorkerProducer;
    this.redisWorkerProducer = redisWorkerProducer;
    this.kafkaWorkerProducer = kafkaWorkerProducer;
  }

  // sampleWorkerPost method that implements the SampleWorkerService interface
  sampleWorkerPost = async (
    dto: SampleWorkerRequestDto
  ): Promise<SampleWorkerResponseDto> => {
    const entity = await SampleWorkerRequestMapper.toEntity(dto, this.em);

    await this.databaseWorkerProducer.enqueueJob(entity);
    await this.bullMqWorkerProducer.enqueueJob(entity);
    await this.redisWorkerProducer.enqueueJob(entity);
    await this.kafkaWorkerProducer.enqueueJob(entity);

    return await SampleWorkerResponseMapper.toDto(entity);
  };
}

import {
  boolean,
  number,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import {
  SampleWorkerEventRecord,
  type ISampleWorkerEventRecord
} from '../../persistence/entities/sampleWorkerRecord.entity';
import { SampleWorkerSchema } from '../schemas/sampleWorker.schema';

// RequestMapper function that maps the request schema to the entity
export const SampleWorkerRequestMapper = requestMapper({
  schemaValidator,
  schema: SampleWorkerSchema,
  entity: SampleWorkerEventRecord,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(SampleWorkerEventRecord, {
        ...dto,
        processed: false,
        retryCount: 0,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

// ResponseMapper function that maps the response schema to the entity
export const SampleWorkerResponseMapper = responseMapper({
  schemaValidator,
  schema: {
    message: string,
    processed: boolean,
    retryCount: number
  },
  entity: SampleWorkerEventRecord,
  mapperDefinition: {
    toDto: async (entity: ISampleWorkerEventRecord) => {
      if (!entity.isInitialized()) {
        throw new Error('SampleWorkerEventRecord is not initialized');
      }
      return wrap(entity).toPOJO();
    }
  }
});

// Exported types for backward compatibility
export type SampleWorkerRequestDto = Parameters<
  typeof SampleWorkerRequestMapper.toEntity
>[0];
export type SampleWorkerResponseDto = Awaited<
  ReturnType<typeof SampleWorkerResponseMapper.toDto>
>;

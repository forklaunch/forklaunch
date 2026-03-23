import {
  requestMapper,
  responseMapper
} from '@forklaunch/core/mappers';
import { schemaValidator } from '@{{app_name}}/core';
import { {{^is_worker}}EntityManager, InferEntity, wrap } from '@mikro-orm/core';{{/is_worker}}{{#is_worker}}InferEntity, wrap } from '@mikro-orm/core';
import type { I{{pascal_case_name}}EventRecord } from '../types/{{camel_case_name}}EventRecord.types';
import { v4 } from 'uuid';{{/is_worker}}
import { {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record } from '../../persistence/entities/{{camel_case_name}}{{#is_worker}}Event{{/is_worker}}Record.entity';
import { {{pascal_case_name}}RequestSchema, {{pascal_case_name}}ResponseSchema } from '../schemas/{{camel_case_name}}.schema';

// RequestMapper const that maps a request schema to an entity
export const {{pascal_case_name}}RequestMapper = requestMapper({
  schemaValidator,
  schema: {{pascal_case_name}}RequestSchema,
  entity: {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record,
  mapperDefinition: {
    toEntity: async (dto{{^is_worker}}, em: EntityManager{{/is_worker}}) => {
      {{^is_worker}}return em.create({{pascal_case_name}}Record, {
        ...dto,
        createdAt: new Date(),
        updatedAt: new Date(),
      });{{/is_worker}}{{#is_worker}}return {
        id: v4(),
        ...dto,
        processed: false,
        retryCount: 0,
        createdAt: new Date(),
        updatedAt: new Date(),
      } satisfies I{{pascal_case_name}}EventRecord;{{/is_worker}}
    }
  }
});

// ResponseMapper const that maps an entity to a response schema
export const {{pascal_case_name}}ResponseMapper = responseMapper({
  schemaValidator,
  schema: {{pascal_case_name}}ResponseSchema,
  entity: {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record,
  mapperDefinition: {
    toDto: async (entity: {{#is_worker}}I{{pascal_case_name}}EventRecord{{/is_worker}}{{^is_worker}}InferEntity<typeof {{pascal_case_name}}Record>{{/is_worker}}) => {
      return wrap(entity).toPOJO();
    }
  }
});

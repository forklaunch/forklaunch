import { {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record } from './entities/{{camel_case_name}}{{#is_worker}}Event{{/is_worker}}Record.entity';
import { InferEntity, RequiredEntityData } from '@mikro-orm/core';
//! Begin seed data
export const {{camel_case_name}}{{#is_worker}}Event{{/is_worker}}RecordData = {
  message: 'Hello, world!'{{#is_worker}},
  processed: false,
  retryCount: 0{{/is_worker}}
} satisfies RequiredEntityData<InferEntity<typeof {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record>>;

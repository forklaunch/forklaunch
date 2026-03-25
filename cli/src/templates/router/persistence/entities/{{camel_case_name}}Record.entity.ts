import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { {{#is_mongo}}nosql{{/is_mongo}}{{^is_mongo}}sql{{/is_mongo}}BaseProperties } from '@{{app_name}}/core';{{#is_worker}}
import type { I{{pascal_case_name}}EventRecord } from '../../domain/types/{{camel_case_name}}EventRecord.types';{{/is_worker}}

export const {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record = defineComplianceEntity({
  name: '{{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record',
  properties: {
    ...{{#is_mongo}}nosql{{/is_mongo}}{{^is_mongo}}sql{{/is_mongo}}BaseProperties,
    message: fp.string().compliance('none'),{{#is_worker}}
    processed: fp.boolean().compliance('none'),
    retryCount: fp.integer().compliance('none'),{{/is_worker}}
  },
});

export type {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record = InferEntity<typeof {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record>;{{#is_worker}}

// Compile-time check: entity type must extend the event record interface
type _AssertEntityExtendsInterface = {{pascal_case_name}}EventRecord extends I{{pascal_case_name}}EventRecord ? true : never;{{/is_worker}}

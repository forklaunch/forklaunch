import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';
import { {{#is_mongo}}nosql{{/is_mongo}}{{^is_mongo}}sql{{/is_mongo}}BaseProperties } from '@{{app_name}}/core';

export const {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record = defineComplianceEntity({
  name: '{{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record',
  properties: {
    ...{{#is_mongo}}nosql{{/is_mongo}}{{^is_mongo}}sql{{/is_mongo}}BaseProperties,
    message: fp.string().compliance('none'),{{#is_worker}}
    tenantId: fp.string().compliance('none'),
    processed: fp.boolean().compliance('none'),
    retryCount: fp.integer().compliance('none'),{{/is_worker}}
  },
});

export type {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record = InferEntity<typeof {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record>;

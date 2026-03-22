import { defineEntity, p } from '@mikro-orm/core';
import { {{#is_mongo}}nosql{{/is_mongo}}{{^is_mongo}}sql{{/is_mongo}}BaseProperties } from '@{{app_name}}/core';

export const {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record = defineEntity({
  name: '{{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record',
  properties: {
    ...{{#is_mongo}}nosql{{/is_mongo}}{{^is_mongo}}sql{{/is_mongo}}BaseProperties,
    message: p.string(),{{#is_worker}}
    processed: p.boolean(),
    retryCount: p.integer(),{{/is_worker}}
  },
});

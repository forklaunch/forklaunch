import { forklaunchRouter, schemaValidator } from '@{{app_name}}/core';
import { {{camel_case_name}}Get, {{camel_case_name}}Post } from '../controllers/{{camel_case_name}}.controller';
import { ci, tokens } from '../../bootstrapper';


// resolve the dependencies
const openTelemetryCollector = ci.resolve(tokens.{{otel_token}});

// defines the router for the {{camel_case_name}} routes
export const {{camel_case_name}}Router = forklaunchRouter(
  '/{{kebab_case_name}}',
  schemaValidator, 
  openTelemetryCollector
);

// mount the routes
export const {{camel_case_name}}GetRoute = {{camel_case_name}}Router.get('/', {{camel_case_name}}Get);
export const {{camel_case_name}}PostRoute = {{camel_case_name}}Router.post('/', {{camel_case_name}}Post);
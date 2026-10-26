import { SchemaValidator } from '@{{app_name}}/core';
import { toolFactory } from '@forklaunch/core/agent';

/**
 * Defines a tool with the same schema descriptors as routes. Set annotations
 * honestly: only `readOnlyHint: true` tools run without the user's approval.
 */
export const defineTool = toolFactory(SchemaValidator());

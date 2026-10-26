import { forklaunchRouter, schemaValidator } from '@{{app_name}}/core';
import { ci, tokens } from '../../bootstrapper';
import {
  agentTask,
  agentTaskStream,
  agentTools
} from '../controllers/agent.controller';

const openTelemetryCollector = ci.resolve(tokens.{{otel_token}});

// The agent contract shared by every ForkLaunch agent, in any language.
export const agentRouter = forklaunchRouter(
  '/api/v1/agent',
  schemaValidator,
  openTelemetryCollector
);

export const agentToolsRoute = agentRouter.get('/tools', agentTools);
export const agentTaskRoute = agentRouter.post('/tasks', agentTask);
export const agentTaskStreamRoute = agentRouter.post(
  '/tasks/stream',
  agentTaskStream
);

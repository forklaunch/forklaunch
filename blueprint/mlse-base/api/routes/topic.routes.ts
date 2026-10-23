import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  assembleTopic,
  getTopic,
  getTopicPhase,
  listTopics
} from '../controllers/topic.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const topicRouter = forklaunchRouter(
  '/topic',
  schemaValidator,
  openTelemetryCollector
);

export const listTopicsRoute = topicRouter.get('/', listTopics);
export const getTopicRoute = topicRouter.get('/:slug', getTopic);
export const getTopicPhaseRoute = topicRouter.get('/:slug/phase/:number', getTopicPhase);
export const assembleTopicRoute = topicRouter.post('/:slug/assemble', assembleTopic);

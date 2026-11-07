import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  listVoiceSettings,
  setVoiceSetting,
  voiceQuery
} from '../controllers/voice.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const voiceRouter = forklaunchRouter(
  '/voice',
  schemaValidator,
  openTelemetryCollector
);

export const voiceQueryRoute = voiceRouter.post('/query', voiceQuery);
export const listVoiceSettingsRoute = voiceRouter.get('/setting', listVoiceSettings);
export const setVoiceSettingRoute = voiceRouter.put('/setting', setVoiceSetting);

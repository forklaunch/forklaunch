import {
  array,
  handlers,
  number,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  InvalidCareAreaError,
  VoiceDisabledError
} from '../../domain/services/voice.service';

const voiceServiceFactory = ci.scopedResolver(tokens.VoiceService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

const hmacAuth = { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } };

const SettingSchema = {
  area: string,
  enabled: schemaValidator.boolean,
  updatedBy: string,
  updatedAt: optional(string)
};

export const voiceQuery = handlers.post(
  schemaValidator,
  '/query',
  {
    name: 'Voice Query',
    access: 'internal',
    summary:
      'Answers a spoken question from a care area where the organization has enabled voice; refused (403) everywhere else. Identifiers are removed from the transcript first; returns a short spoken summary and the answer id',
    auth: hmacAuth,
    body: {
      organizationId: string,
      area: string,
      transcript: string,
      userId: optional(string)
    },
    responses: {
      200: {
        spokenSummary: string,
        identifiersRemoved: number,
        answerId: string,
        kind: string,
        queryClass: string
      },
      400: string,
      403: string
    }
  },
  async (req, res) => {
    if (!req.body.transcript.trim() || req.body.transcript.length > 2000) {
      res.status(400).send('transcript must be 1 to 2000 characters');
      return;
    }
    try {
      const result = await voiceServiceFactory().query(req.body);
      res.status(200).json({
        spokenSummary: result.spokenSummary,
        identifiersRemoved: result.identifiersRemoved,
        answerId: result.answer.answerId,
        kind: result.answer.kind,
        queryClass: result.answer.queryClass
      });
    } catch (error) {
      if (error instanceof VoiceDisabledError) {
        res.status(403).send(`${error.message}. Typed search is always available.`);
        return;
      }
      throw error;
    }
  }
);

export const listVoiceSettings = handlers.get(
  schemaValidator,
  '/setting',
  {
    name: 'List Voice Settings',
    access: 'internal',
    summary: 'Voice settings per care area for an organization; areas not listed are off',
    auth: hmacAuth,
    query: { organizationId: string },
    responses: { 200: array(SettingSchema) }
  },
  async (req, res) => {
    res.status(200).json(await voiceServiceFactory().listSettings(req.query.organizationId));
  }
);

export const setVoiceSetting = handlers.put(
  schemaValidator,
  '/setting',
  {
    name: 'Set Voice Setting',
    access: 'internal',
    summary:
      'Turns voice on or off for one care area of an organization, recording the decision and who made it',
    auth: hmacAuth,
    body: {
      organizationId: string,
      area: string,
      enabled: schemaValidator.boolean,
      updatedBy: string
    },
    responses: { 200: SettingSchema, 400: string }
  },
  async (req, res) => {
    try {
      const { organizationId, area, enabled, updatedBy } = req.body;
      res.status(200).json(await voiceServiceFactory().setSetting(organizationId, area, enabled, updatedBy));
    } catch (error) {
      if (error instanceof InvalidCareAreaError) {
        res.status(400).send(error.message);
        return;
      }
      throw error;
    }
  }
);

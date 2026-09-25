import {
  array,
  handlers,
  number,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { TopicNotFoundError } from '../../domain/services/topic.service';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const topicServiceFactory = ci.scopedResolver(tokens.TopicService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

const hmacAuth = {
  hmac: {
    secretKeys: {
      default: HMAC_SECRET_KEY
    }
  }
};

const EvidenceSchema = {
  passageId: string,
  sourceKey: string,
  externalId: string,
  title: string,
  url: string,
  publishedAt: optional(string),
  isCaseReport: schemaValidator.boolean,
  licenseScope: string,
  sectionPath: string,
  text: string
};

const FactSchema = {
  raw: string,
  low: number,
  high: number,
  unit: string,
  statistic: optional(string),
  sentence: string,
  reviewStatus: string,
  passageId: string
};

const ItemSchema = {
  key: string,
  label: string,
  number: optional(number),
  status: string,
  evidence: array(EvidenceSchema),
  facts: array(FactSchema)
};

const TopicPageSchema = {
  slug: string,
  title: string,
  topicType: string,
  status: string,
  framework: { key: string, status: string },
  notice: optional(string),
  assembledAt: optional(string),
  questions: array(ItemSchema),
  phases: array(ItemSchema),
  caseStudies: array({
    diagnosis: string,
    evidenceLevel: string,
    cases: array({
      sourceKey: string,
      externalId: string,
      title: string,
      url: string,
      publishedAt: optional(string),
      licenseScope: string,
      relevanceReason: string,
      presentation: optional(string),
      diagnosis: optional(string),
      management: optional(string),
      outcome: optional(string)
    })
  })
};

export const listTopics = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Topics',
    access: 'internal',
    summary: 'Lists topic pages with their approval status',
    auth: hmacAuth,
    responses: {
      200: array({
        slug: string,
        title: string,
        topicType: string,
        status: string,
        assembledAt: optional(string)
      })
    }
  },
  async (_req, res) => {
    res.status(200).json(await topicServiceFactory().listTopics());
  }
);

export const getTopic = handlers.get(
  schemaValidator,
  '/:slug',
  {
    name: 'Get Topic',
    access: 'internal',
    summary:
      'A topic page: every framework question and procedure phase with its cited evidence or an explicit insufficient-evidence state, extracted numbers for review, and case studies grouped by diagnosis',
    auth: hmacAuth,
    params: { slug: string },
    responses: {
      200: TopicPageSchema,
      404: string
    }
  },
  async (req, res) => {
    try {
      res.status(200).json(await topicServiceFactory().getPage(req.params.slug));
    } catch (error) {
      if (error instanceof TopicNotFoundError) {
        res.status(404).send(error.message);
        return;
      }
      throw error;
    }
  }
);

export const getTopicPhase = handlers.get(
  schemaValidator,
  '/:slug/phase/:number',
  {
    name: 'Get Topic Phase',
    access: 'internal',
    summary:
      'One phase of a procedure walkthrough, so a surgeon can jump straight to it',
    auth: hmacAuth,
    params: { slug: string, number: string },
    responses: {
      200: TopicPageSchema,
      400: string,
      404: string
    }
  },
  async (req, res) => {
    const phase = Number(req.params.number);
    if (!Number.isInteger(phase) || phase < 1 || phase > 20) {
      res.status(400).send('phase number must be an integer from 1 to 20');
      return;
    }
    try {
      const page = await topicServiceFactory().getPage(req.params.slug, phase);
      if (page.phases.length === 0) {
        res.status(404).send(`Topic '${req.params.slug}' has no phase ${phase}`);
        return;
      }
      res.status(200).json(page);
    } catch (error) {
      if (error instanceof TopicNotFoundError) {
        res.status(404).send(error.message);
        return;
      }
      throw error;
    }
  }
);

export const assembleTopic = handlers.post(
  schemaValidator,
  '/:slug/assemble',
  {
    name: 'Assemble Topic',
    access: 'internal',
    summary:
      'Rebuilds a topic page from the stored corpus: evidence per question and phase, numbers for review, and case studies',
    auth: hmacAuth,
    params: { slug: string },
    responses: {
      200: {
        slug: string,
        items: number,
        withEvidence: number,
        insufficientEvidence: array(string),
        facts: number,
        caseStudies: number,
        casesExcluded: number
      },
      404: string
    }
  },
  async (req, res) => {
    try {
      const result = await topicServiceFactory().assemble(req.params.slug);
      openTelemetryCollector.info('Topic assembled via API', result);
      res.status(200).json(result);
    } catch (error) {
      if (error instanceof TopicNotFoundError) {
        res.status(404).send(error.message);
        return;
      }
      throw error;
    }
  }
);

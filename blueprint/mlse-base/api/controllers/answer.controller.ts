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

const answerServiceFactory = ci.scopedResolver(tokens.AnswerService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

const hmacAuth = {
  hmac: {
    secretKeys: {
      default: HMAC_SECRET_KEY
    }
  }
};

const AnswerBodySchema = {
  query: string,
  topicSlug: optional(string),
  live: optional(schemaValidator.boolean),
  // enables the organization's licensed sources; with userId, records the
  // search in the user's history
  organizationId: optional(string),
  userId: optional(string)
};

const SectionSchema = {
  key: string,
  label: string,
  number: optional(number),
  status: string,
  sentences: array({
    text: string,
    citations: array(string),
    quoted: optional(schemaValidator.boolean)
  }),
  removed: number
};

const AnswerSchema = {
  answerId: string,
  query: string,
  queryClass: string,
  kind: string,
  message: optional(string),
  notice: string,
  sections: array(SectionSchema),
  sources: array({
    passageId: string,
    origin: string,
    sourceKey: string,
    externalId: string,
    title: string,
    url: string,
    publishedAt: optional(string),
    isCaseReport: schemaValidator.boolean,
    licenseScope: string,
    sectionPath: string,
    text: string
  }),
  model: optional(string)
};

const bodyValid = (query: string) => query.trim().length > 0 && query.length <= 2000;

export const answer = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Answer',
    access: 'internal',
    summary:
      'Streams an answer section by section as each is verified: a start event with the query classification, one event per section, and a done event with the full answer and its sources',
    auth: hmacAuth,
    body: AnswerBodySchema,
    responses: {
      200: {
        contentType: 'text/event-stream',
        event: {
          id: string,
          data: {
            type: string,
            queryClass: optional(string),
            kind: optional(string),
            message: optional(string),
            section: optional(SectionSchema),
            answer: optional(AnswerSchema)
          }
        }
      },
      400: string
    }
  },
  async (req, res) => {
    if (!bodyValid(req.body.query)) {
      res.status(400).send('query must be 1 to 2000 characters');
      return;
    }
    const service = answerServiceFactory();
    res.status(200).sseEmitter(async function* () {
      let id = 0;
      for await (const event of service.stream(req.body)) {
        yield { id: String(id++), data: event };
      }
    });
  }
);

export const answerComplete = handlers.post(
  schemaValidator,
  '/complete',
  {
    name: 'Answer Complete',
    access: 'internal',
    summary:
      'The same answer as POST /answer, returned once every section is verified',
    auth: hmacAuth,
    body: AnswerBodySchema,
    responses: {
      200: AnswerSchema,
      400: string,
      404: string
    }
  },
  async (req, res) => {
    if (!bodyValid(req.body.query)) {
      res.status(400).send('query must be 1 to 2000 characters');
      return;
    }
    try {
      res.status(200).json(await answerServiceFactory().answer(req.body));
    } catch (error) {
      if (error instanceof TopicNotFoundError) {
        res.status(404).send(error.message);
        return;
      }
      throw error;
    }
  }
);

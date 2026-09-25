import {
  array,
  handlers,
  IdSchema,
  number,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { Document } from '../../persistence/entities/document.entity';
import { DocumentChunk } from '../../persistence/entities/documentChunk.entity';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const entityManagerFactory = ci.scopedResolver(tokens.EntityManager);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

export const getDocument = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Document',
    access: 'internal',
    summary:
      'Returns one stored document version with its license scope, status and passages',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: IdSchema,
    responses: {
      200: {
        id: string,
        sourceKey: string,
        externalId: string,
        version: number,
        title: string,
        url: string,
        publishedAt: optional(string),
        license: optional(string),
        licenseScope: string,
        isCaseReport: schemaValidator.boolean,
        status: string,
        meshDescriptorUis: array(string),
        passages: array({
          sectionPath: string,
          ordinal: number,
          text: string
        })
      },
      404: string
    }
  },
  async (req, res) => {
    const em = entityManagerFactory();
    const document = await em.findOne(Document, { id: req.params.id });
    if (!document) {
      res.status(404).send(`Document '${req.params.id}' not found`);
      return;
    }

    const chunks = await em.find(
      DocumentChunk,
      { document: document.id },
      { orderBy: { ordinal: 'asc' } }
    );
    openTelemetryCollector.debug('Fetched document', {
      id: document.id,
      passages: chunks.length
    });

    res.status(200).json({
      id: document.id,
      sourceKey: document.sourceKey,
      externalId: document.externalId,
      version: document.version,
      title: document.title,
      url: document.url,
      publishedAt: document.publishedAt ?? undefined,
      license: document.license ?? undefined,
      licenseScope: document.licenseScope,
      isCaseReport: document.isCaseReport,
      status: document.status,
      meshDescriptorUis: document.meshDescriptorUis,
      passages: chunks.map((chunk) => ({
        sectionPath: chunk.sectionPath,
        ordinal: chunk.ordinal,
        text: chunk.text
      }))
    });
  }
);

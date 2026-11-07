import { MeshDescriptorParser } from '@forklaunch/implementation-mlse-base/services';
import { MeshConceptDto } from '@forklaunch/interfaces-mlse/types';
import { createReadStream } from 'node:fs';
import { ci, tokens } from '../bootstrapper';

/**
 * Loads NLM's annual MeSH descriptor file (descYYYY.xml, public domain) into
 * the medical_concept table. The file is several hundred megabytes, so it is
 * streamed and written in batches; re-running with a newer year's file
 * updates existing descriptors in place.
 *
 *   pnpm mesh:load path/to/desc2026.xml
 */
const BATCH_SIZE = 500;

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

async function main() {
  const file = process.argv[2];
  if (!file) {
    throw new Error('Usage: mesh:load <path to descYYYY.xml>');
  }

  const parser = new MeshDescriptorParser();
  let pending: MeshConceptDto[] = [];
  let loaded = 0;

  const flush = async () => {
    if (pending.length === 0) {
      return;
    }
    // a fresh scoped EntityManager per batch keeps the identity map small
    loaded += await ci.scopedResolver(tokens.IngestionService)().loadMeshConcepts(pending);
    pending = [];
    openTelemetryCollector.info('MeSH descriptors loaded', { loaded });
  };

  for await (const chunk of createReadStream(file, { encoding: 'utf-8' })) {
    pending.push(...parser.push(chunk as string));
    if (pending.length >= BATCH_SIZE) {
      await flush();
    }
  }
  await flush();
  openTelemetryCollector.info('MeSH load complete', { loaded });
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error('[load-mesh] Fatal error', error);
    process.exit(1);
  });

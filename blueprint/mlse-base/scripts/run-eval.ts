import {
  GoldQuestion,
  retrievalCompleteness,
  summarizeEvaluation
} from '@forklaunch/implementation-mlse-base/services';
import { readFileSync } from 'node:fs';
import { ci, tokens } from '../bootstrapper';

/**
 * Measures retrieval against a gold set: for every question, the share of
 * the documents a clinician expects that appear in the top-k results.
 * Run before any change to retrieval, prompts or the AI provider.
 *
 *   pnpm eval:run path/to/gold-set.json [k=10] [--live]
 *
 * Gold set format: see eval/gold-set.example.json. The real gold set is
 * written and approved by clinicians, not generated.
 */
async function main() {
  const [file, kArg, ...flags] = process.argv.slice(2);
  if (!file) {
    throw new Error('Usage: eval:run <gold-set.json> [k] [--live]');
  }
  const k = kArg && !kArg.startsWith('--') ? Number(kArg) : 10;
  const live = [kArg, ...flags].includes('--live');
  const { questions } = JSON.parse(readFileSync(file, 'utf-8')) as {
    questions: GoldQuestion[];
  };

  const scores = [];
  for (const question of questions) {
    const response = await ci
      .scopedResolver(tokens.SearchService)()
      .search({ query: question.query, limit: k, live });
    // one document may be cited through any of its passages
    const documents = [
      ...new Map(
        response.results.map((r) => [
          `${r.sourceKey}:${r.externalId}`,
          { sourceKey: r.sourceKey, externalId: r.externalId }
        ])
      ).values()
    ];
    const score = retrievalCompleteness(question, documents, k);
    scores.push(score);
    console.log(
      `${score.completeness === 1 ? 'PASS' : 'MISS'}  ${question.id}  ${score.found}/${score.expected}  ${question.query}` +
        (score.missing.length
          ? `\n      missing: ${score.missing.map((m) => `${m.sourceKey}:${m.externalId}`).join(', ')}`
          : '')
    );
  }

  const summary = summarizeEvaluation(scores);
  console.log(
    `\nRetrieval completeness @${k}: ${(summary.meanCompleteness * 100).toFixed(1)}% ` +
      `(${summary.fullyAnswered}/${summary.questions} questions fully answered; target 85–90%)`
  );
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error('[run-eval] Fatal error', error);
    process.exit(1);
  });

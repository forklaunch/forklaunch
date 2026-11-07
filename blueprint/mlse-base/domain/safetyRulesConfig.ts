import {
  parseSafetyRuleAdditions,
  useSafetyRuleAdditions,
  type SafetyRules
} from '@forklaunch/implementation-mlse-base/services';
import { readFileSync } from 'node:fs';

/**
 * Applies a deployment's own safety phrases (MLSE_SAFETY_RULES_FILE, a JSON
 * file of list names to phrases) on top of the built-in ones. Additions can
 * only make the classifier more careful. An unreadable or malformed file
 * stops the service at startup: running with fewer rules than the
 * deployment asked for must never happen quietly.
 */
export function applySafetyRulesFile(
  path: string | undefined,
  readFile: (path: string) => string = (p) => readFileSync(p, 'utf8')
): SafetyRules | undefined {
  if (!path) return undefined;
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFile(path));
  } catch (error) {
    throw new Error(
      `MLSE_SAFETY_RULES_FILE '${path}' could not be read as JSON: ${(error as Error).message}`
    );
  }
  return useSafetyRuleAdditions(parseSafetyRuleAdditions(parsed));
}

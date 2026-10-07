import {
  activeSafetyRules,
  classifyQuery,
  DEFAULT_SAFETY_RULES,
  useSafetyRuleAdditions
} from '@forklaunch/implementation-mlse-base/services';
import { applySafetyRulesFile } from '../domain/safetyRulesConfig';

describe('MLSE_SAFETY_RULES_FILE', () => {
  afterEach(() => {
    // back to the built-in rules for the next test
    useSafetyRuleAdditions({});
  });

  it('does nothing when unset', () => {
    expect(applySafetyRulesFile(undefined)).toBeUndefined();
    expect(activeSafetyRules()).toEqual(DEFAULT_SAFETY_RULES);
  });

  it("applies a deployment's additions to every classification", () => {
    const query = 'my patient drank hand sanitizer';
    expect(classifyQuery(query).queryClass).toBe('patient_specific_treatment');

    const rules = applySafetyRulesFile('/etc/mlse/safety.json', () =>
      JSON.stringify({ version: 'site-1', poisons: ['hand sanitizer'] })
    );

    expect(rules?.version).toBe(`${DEFAULT_SAFETY_RULES.version}+site-1`);
    expect(classifyQuery(query).queryClass).toBe('emergency_pattern');
  });

  it('stops startup on a file that is not JSON, or has an unknown list', () => {
    expect(() =>
      applySafetyRulesFile('/etc/mlse/safety.json', () => '{ poisons: [')
    ).toThrow(
      "MLSE_SAFETY_RULES_FILE '/etc/mlse/safety.json' could not be read as JSON"
    );
    expect(() =>
      applySafetyRulesFile('/etc/mlse/safety.json', () =>
        JSON.stringify({ poison: ['bleach'] })
      )
    ).toThrow('unknown list "poison"');
    expect(activeSafetyRules()).toEqual(DEFAULT_SAFETY_RULES);
  });
});

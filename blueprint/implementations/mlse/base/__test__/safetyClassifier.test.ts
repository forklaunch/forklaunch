import {
  DEFAULT_SAFETY_RULES,
  extendSafetyRules,
  parseSafetyRuleAdditions
} from '../domain/safetyRules';
import { classifyQuery } from '../services/queryClassifier.service';
import { SAFETY_PHRASINGS } from './fixtures/safetyPhrasings';

describe('safety classifier, on the evaluation set', () => {
  for (const { group, expected, queries } of SAFETY_PHRASINGS) {
    it(`${group} -> ${expected}`, () => {
      for (const query of queries) {
        expect([
          query,
          classifyQuery(query, DEFAULT_SAFETY_RULES).queryClass
        ]).toEqual([query, expected]);
      }
    });
  }
});

describe('safety rule additions', () => {
  it('a deployment can add phrases, which then apply', () => {
    const query = 'my patient drank hand sanitizer';
    expect(classifyQuery(query, DEFAULT_SAFETY_RULES).queryClass).toBe(
      'patient_specific_treatment'
    );

    const rules = extendSafetyRules(
      DEFAULT_SAFETY_RULES,
      parseSafetyRuleAdditions({
        version: 'site-1',
        poisons: ['hand sanitizer'],
        _note: 'local'
      })
    );
    expect(rules.version).toBe(`${DEFAULT_SAFETY_RULES.version}+site-1`);
    expect(classifyQuery(query, rules).queryClass).toBe('emergency_pattern');
  });

  it('additions never remove a built-in phrase', () => {
    const rules = extendSafetyRules(
      DEFAULT_SAFETY_RULES,
      parseSafetyRuleAdditions({ acuteEvents: [] })
    );
    expect(rules.acuteEvents).toEqual(DEFAULT_SAFETY_RULES.acuteEvents);
  });

  it('refuses a misspelled list name, so a rule cannot be silently dropped', () => {
    expect(() => parseSafetyRuleAdditions({ acuteEvent: ['x'] })).toThrow(
      'unknown list "acuteEvent"'
    );
    expect(() => parseSafetyRuleAdditions({ poisons: 'bleach' })).toThrow(
      'array of non-empty phrases'
    );
    expect(() => parseSafetyRuleAdditions(['bleach'])).toThrow('JSON object');
  });

  it('every built-in phrase is already normalized, so each one can match', () => {
    for (const [key, list] of Object.entries(DEFAULT_SAFETY_RULES)) {
      if (!Array.isArray(list)) continue;
      for (const phrase of list) {
        expect([key, phrase]).toEqual([
          key,
          phrase
            .toLowerCase()
            .replace(/[^a-z0-9.]+/g, ' ')
            .trim()
        ]);
      }
    }
  });
});

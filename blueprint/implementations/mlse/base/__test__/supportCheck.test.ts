import { verifyDraft } from '../services/citationVerifier.service';
import { checkSupport } from '../services/supportCheck.service';

// The sentences from the #394 review that word overlap alone kept, each
// against a passage that says something different, plus faithful versions
// of the same claims that must still pass.
const verdict = (sentence: string, source: string) => checkSupport(sentence, [source]).ok;

describe('checkSupport: rejects sentences that contradict their source', () => {
  it('a negation the source does not make, or one it makes that the sentence drops', () => {
    const source = 'In the trial, aspirin reduced mortality compared with placebo.';
    expect(verdict('Aspirin did not reduce mortality.', source)).toBe(false);
    expect(verdict('Aspirin reduced mortality.', source)).toBe(true);

    const negated = 'Aspirin did not reduce mortality in older adults.';
    expect(verdict('Aspirin reduced mortality in older adults.', negated)).toBe(false);
    expect(verdict("Aspirin didn't reduce mortality in older adults.", negated)).toBe(true);
  });

  it('numbers swapped between groups', () => {
    const source = 'Mortality was 5% in the aspirin group and 20% in the placebo group.';
    expect(verdict('Mortality was 20% in the aspirin group and 5% in the placebo group.', source)).toBe(false);
    expect(verdict('Mortality was 5% with aspirin and 20% with placebo.', source)).toBe(true);
  });

  it('a reversed comparison', () => {
    const source = 'Warfarin was superior to aspirin for stroke prevention.';
    expect(verdict('Aspirin was superior to warfarin for stroke prevention.', source)).toBe(false);
    expect(verdict('Aspirin was inferior to warfarin for stroke prevention.', source)).toBe(true);
    expect(verdict('Warfarin was superior to aspirin for stroke prevention.', source)).toBe(true);
  });

  it('the opposite word: recommended where the source says contraindicated', () => {
    const source = 'Treatment is contraindicated in renal failure.';
    expect(verdict('Treatment is recommended in renal failure.', source)).toBe(false);
    expect(verdict('Treatment is not recommended... it is contraindicated in renal failure.', source)).toBe(true);
  });

  it('a dose given with another dose schedule', () => {
    const source = 'Give 500 mg twice daily, or 1 g every 8 hours for 7 days in severe infection.';
    expect(verdict('Give 500 mg every 8 hours for 7 days.', source)).toBe(false);
    expect(verdict('Give 500 mg twice daily.', source)).toBe(true);
    expect(verdict('In severe infection, give 1 g every 8 hours for 7 days.', source)).toBe(true);
  });

  it('a long run of digits with no unit is handled in linear time', () => {
    const digits = '0'.repeat(50_000);
    const started = Date.now();
    expect(verdict(`Give ${digits} every 8 hours.`, `Give ${digits} twice daily.`)).toBe(true);
    expect(Date.now() - started).toBeLessThan(1000);
  });
});

describe('verifyDraft with the support check', () => {
  it('removes each contradicting sentence and keeps the faithful ones', () => {
    const passages = new Map([
      ['P1', 'Aspirin reduced mortality compared with placebo. Mortality was 5% in the aspirin group and 20% in the placebo group.'],
      ['P2', 'Treatment should continue for 2 weeks and is contraindicated in renal failure.']
    ]);
    const result = verifyDraft(
      [
        'Aspirin did not reduce mortality compared with placebo [P1].',
        'Mortality was 5% in the aspirin group and 20% in the placebo group [P1].',
        'Treatment should continue for 2 months [P2].',
        'Treatment is recommended in renal failure [P2].',
        'Treatment should continue for 2 weeks [P2].'
      ].join('\n'),
      passages
    );
    expect(result.kept.map((s) => s.text)).toEqual([
      'Mortality was 5% in the aspirin group and 20% in the placebo group.',
      'Treatment should continue for 2 weeks.'
    ]);
    expect(result.removed.map((r) => r.text)).toEqual([
      'Aspirin did not reduce mortality compared with placebo.',
      'Treatment should continue for 2 months.',
      'Treatment is recommended in renal failure.'
    ]);
  });
});

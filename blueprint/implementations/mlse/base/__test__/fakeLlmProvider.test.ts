import { FakeLlmProvider } from '../services/fakeLlmProvider.service';

describe('FakeLlmProvider', () => {
  const provider = new FakeLlmProvider(4);

  it('only restates the evidence it is given, citing each passage', async () => {
    const result = await provider.generate({
      instructions: 'Answer from evidence only.',
      prompt: 'What is the typical blood loss?',
      evidence: [
        { id: 'S1', text: 'Median blood loss was under 50 mL.' },
        { id: 'S2', text: 'Transfusion was rare.' }
      ]
    });
    expect(result.text).toBe(
      'Median blood loss was under 50 mL. [S1] Transfusion was rare. [S2]'
    );
  });

  it('returns an empty answer when there is no evidence', async () => {
    const result = await provider.generate({
      instructions: '',
      prompt: 'Anything?',
      evidence: []
    });
    expect(result.text).toBe('');
  });

  it('produces stable unit vectors of the configured size', async () => {
    const first = await provider.embed({ texts: ['cholecystectomy', 'hsct'] });
    const second = await provider.embed({ texts: ['cholecystectomy'] });

    expect(first.dimensions).toBe(4);
    expect(first.embeddings).toHaveLength(2);
    expect(first.embeddings[0]).toEqual(second.embeddings[0]);
    const norm = Math.sqrt(
      first.embeddings[0].reduce((sum, v) => sum + v * v, 0)
    );
    expect(norm).toBeCloseTo(1, 10);
  });

  it('rejects an invalid dimension', () => {
    expect(() => new FakeLlmProvider(0)).toThrow();
  });
});

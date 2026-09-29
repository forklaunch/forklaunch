// Type-checking resolves workspace packages through their declaration files,
// which keep the source directory layout, so a wrong runtime entry in a
// package's `exports` map passes `tsc` and only fails when Node loads the
// module. Importing the built packages here exercises the real resolution.
describe('mlse package exports resolve at runtime', () => {
  it('loads @forklaunch/implementation-mlse-base/services', async () => {
    const services = await import('@forklaunch/implementation-mlse-base/services');
    expect(typeof services.PublicCorpusProvider).toBe('function');
    expect(typeof services.FakeLlmProvider).toBe('function');
    expect(services.licenseScopeFor('CC BY 4.0')).toBe('full_text');
  });

  it('loads @forklaunch/interfaces-mlse', async () => {
    await expect(
      import('@forklaunch/interfaces-mlse/types')
    ).resolves.toBeDefined();
    await expect(
      import('@forklaunch/interfaces-mlse/interfaces')
    ).resolves.toBeDefined();
  });
});

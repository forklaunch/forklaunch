import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';

/**
 * The codemod rewrites reads of compliant fields to `.deanon`, leaves entity
 * data and `where` values alone, and reports what it cannot decide.
 */

const fixtures = path.join(__dirname, 'fixtures');
// Same depth as the fixture, so its relative import of src still resolves.
const workdir = mkdtempSync(path.join(fixtures, 'codemod-run-'));
cpSync(path.join(fixtures, 'codemod'), workdir, { recursive: true });

afterAll(() => rmSync(workdir, { recursive: true, force: true }));

describe('migrate-compliant-fields', () => {
  const run = spawnSync(
    process.execPath,
    [
      path.join(__dirname, '..', 'bin', 'migrate-compliant-fields.mjs'),
      path.join(workdir, 'tsconfig.json'),
      '--typescript',
      path.dirname(require.resolve('typescript-5/package.json'))
    ],
    { encoding: 'utf8', cwd: workdir }
  );
  const output = readFileSync(path.join(workdir, 'sms.ts'), 'utf8');

  it('rewrites every read of a compliant field to .deanon', () => {
    expect(output).toContain('return { to: r.to.deanon, body: r.body.deanon, status: r.status };');
    expect(output).toContain('send(r.to.deanon, `Message: ${r.body.deanon}`);');
    expect(output).toContain('return r.to.deanon.toLowerCase();');
    expect(output).toContain('return r.to.deanon === other;');
    expect(output).toContain('const note: string | undefined = r.note?.deanon;');
  });

  it('leaves .anon, entity data and where values alone', () => {
    expect(output).toContain('return r.to.anon;');
    expect(output).toContain("to: '+15550100',");
    expect(output).toContain('return em.find(SmsRecord, { to });');
    expect(output).not.toContain('deanon.deanon');
  });

  it('reports a where on a field that is not queryable, and exits non-zero', () => {
    expect(run.stdout).toMatch(/7 read\(s\) +rewritten/);
    expect(run.stdout).toMatch(/sms\.ts:\d+:\d+/);
    expect(run.stdout).toContain('queryable: true');
    expect(run.status).toBe(2);
  });
});

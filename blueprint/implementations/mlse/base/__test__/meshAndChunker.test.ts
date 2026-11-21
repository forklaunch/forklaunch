import { readFileSync } from 'node:fs';
import * as path from 'node:path';
import { chunkSections, contentHash } from '../services/chunker.service';
import { MeshDescriptorParser } from '../services/meshDescriptorParser.service';

const meshXml = readFileSync(
  path.join(__dirname, 'fixtures', 'mesh-descriptors.xml'),
  'utf-8'
);

describe('MeshDescriptorParser', () => {
  it('parses descriptors with synonyms and tree numbers', () => {
    const [lapChole, hsct] = MeshDescriptorParser.parseAll(meshXml);
    expect(lapChole).toEqual({
      descriptorUi: 'D017081',
      preferredTerm: 'Cholecystectomy, Laparoscopic',
      synonyms: ['Laparoscopic Cholecystectomy', 'Celioscopic Cholecystectomy'],
      treeNumbers: ['E04.210.240.130.500', 'E04.502.250.500']
    });
    expect(hsct.descriptorUi).toBe('D018380');
    expect(hsct.synonyms).toContain('Stem Cell Transplantation, Hematopoietic');
  });

  // The real file is several hundred megabytes and is streamed in chunks, so
  // a record split across chunk boundaries must still parse.
  it('gives the same result when fed in small chunks', () => {
    const parser = new MeshDescriptorParser();
    const streamed = [];
    for (let i = 0; i < meshXml.length; i += 97) {
      streamed.push(...parser.push(meshXml.slice(i, i + 97)));
    }
    expect(streamed).toEqual(MeshDescriptorParser.parseAll(meshXml));
  });
});

describe('chunkSections', () => {
  it('never mixes sections and keeps their paths', () => {
    const passages = chunkSections([
      { path: 'Dosage and Administration', text: 'Give 2 g IV. Repeat every 8 hours.' },
      { path: 'Contraindications', text: 'Known hypersensitivity to cephalosporins.' }
    ]);
    expect(passages).toEqual([
      { sectionPath: 'Dosage and Administration', ordinal: 0, text: 'Give 2 g IV. Repeat every 8 hours.' },
      { sectionPath: 'Contraindications', ordinal: 1, text: 'Known hypersensitivity to cephalosporins.' }
    ]);
  });

  it('splits long sections at sentence boundaries within the size limit', () => {
    const sentence = 'The Critical View of Safety was achieved before division.';
    const text = Array.from({ length: 30 }, () => sentence).join(' ');
    const passages = chunkSections([{ path: 'Methods', text }], { maxChars: 200 });
    expect(passages.length).toBeGreaterThan(1);
    for (const passage of passages) {
      expect(passage.text.length).toBeLessThanOrEqual(200);
      expect(passage.text.endsWith('.')).toBe(true);
    }
    expect(passages.map((p) => p.ordinal)).toEqual(passages.map((_, i) => i));
  });

  it('keeps abbreviations such as "e.g." inside a sentence', () => {
    const passages = chunkSections(
      [{ path: 'A', text: 'Use prophylaxis, e.g. cefazolin, before incision. Then proceed.' }],
      { maxChars: 50 }
    );
    expect(passages[0].text).toBe('Use prophylaxis, e.g. cefazolin, before incision.');
  });

  it('splits a single overlong sentence at word boundaries', () => {
    const passages = chunkSections(
      [{ path: 'A', text: 'word '.repeat(100).trim() }],
      { maxChars: 50 }
    );
    expect(passages.every((p) => p.text.length <= 50)).toBe(true);
    expect(passages.map((p) => p.text).join(' ')).toBe('word '.repeat(100).trim());
  });
});

describe('contentHash', () => {
  it('changes when any section changes and not otherwise', () => {
    const base = [{ path: 'A', text: 'one' }];
    expect(contentHash('t', base)).toBe(contentHash('t', [{ path: 'A', text: 'one' }]));
    expect(contentHash('t', base)).not.toBe(contentHash('t', [{ path: 'A', text: 'two' }]));
    expect(contentHash('t', base)).not.toBe(contentHash('t', [{ path: 'B', text: 'one' }]));
    expect(contentHash('t', base)).not.toBe(contentHash('u', base));
  });
});

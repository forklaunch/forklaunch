import {
  CONDITION_FRAMEWORK,
  frameworkItems,
  MEDICATION_FRAMEWORK,
  PROCEDURE_FRAMEWORK,
  QUESTION_FRAMEWORKS
} from '../domain/questionFrameworks';
import {
  caseRelevance,
  diagnosisGroup,
  extractCaseStudyFields
} from '../services/caseStudy.service';
import { selectEvidence } from '../services/evidenceSelection.service';
import { extractQuantities } from '../services/quantityExtractor.service';

describe('extractQuantities', () => {
  it('finds a statistic with its unit and sentence', () => {
    const [q] = extractQuantities('Median blood loss was 20 mL (IQR 10–50). Operative time was long.');
    expect(q).toEqual({
      raw: '20 mL',
      low: 20,
      high: 20,
      unit: 'mL',
      statistic: 'median',
      sentence: 'Median blood loss was 20 mL (IQR 10–50).'
    });
  });

  it('reads ranges written with a dash or "to"', () => {
    expect(extractQuantities('Propofol 1.5–2.5 mg/kg is given for induction.')[0]).toMatchObject({
      raw: '1.5–2.5 mg/kg',
      low: 1.5,
      high: 2.5,
      unit: 'mg/kg'
    });
    expect(extractQuantities('Pneumoperitoneum was kept at 12 to 14 mmHg.')[0]).toMatchObject({
      low: 12,
      high: 14,
      unit: 'mmHg'
    });
  });

  it('reads percentages and durations', () => {
    const found = extractQuantities('Conversion occurred in 7.5% of cases; median stay was 2 days.');
    expect(found.map((f) => [f.raw, f.unit, f.statistic])).toEqual([
      ['7.5%', '%', 'percent'],
      ['2 days', 'days', 'median']
    ]);
  });

  it('ignores numbers that are not measurements', () => {
    expect(
      extractQuantities(
        'Minimal bleeding in PMC123 and D017081. A 3-day course was given to 12 patients in 2026.'
      )
    ).toEqual([]);
  });

  it('runs in linear time on hostile input', () => {
    const start = performance.now();
    extractQuantities('1-'.repeat(50_000) + '5 mL');
    expect(performance.now() - start).toBeLessThan(500);
  });
});

describe('extractCaseStudyFields', () => {
  it("maps a case report's own sections onto the four parts", () => {
    const fields = extractCaseStudyFields([
      { path: 'Case presentation', text: 'A 54-year-old presented with right upper quadrant pain.' },
      { path: 'Investigations', text: 'Ultrasound showed a distended gallbladder.' },
      { path: 'Management', text: 'Emergency laparoscopic cholecystectomy was performed.' },
      { path: 'Outcome and follow-up', text: 'Discharged on day 2.' },
      { path: 'Acknowledgements', text: 'Thanks to the team.' }
    ]);
    expect(fields).toEqual({
      presentation: 'A 54-year-old presented with right upper quadrant pain.',
      diagnosis: 'Ultrasound showed a distended gallbladder.',
      management: 'Emergency laparoscopic cholecystectomy was performed.',
      outcome: 'Discharged on day 2.'
    });
  });

  it('leaves unlabelled parts empty rather than guessing', () => {
    expect(extractCaseStudyFields([{ path: 'Abstract', text: 'Text.' }])).toEqual({});
  });
});

describe('caseRelevance', () => {
  const topic = {
    meshDescriptorUi: 'D017081',
    searchTerms: ['laparoscopic cholecystectomy']
  };

  it('accepts cases tagged with the topic descriptor', () => {
    expect(caseRelevance(topic, { meshDescriptorUis: ['D017081'], title: 'x', text: '' })).toEqual({
      relevant: true,
      reason: 'mesh'
    });
  });

  it('accepts cases that name the topic', () => {
    expect(
      caseRelevance(topic, {
        meshDescriptorUis: [],
        title: 'Bile leak after laparoscopic cholecystectomy',
        text: ''
      }).reason
    ).toBe('terms');
  });

  it('does not count a passing mention in the body', () => {
    expect(
      caseRelevance(topic, {
        meshDescriptorUis: [],
        title: 'Neonatal diarrhea',
        text: 'The mother had a laparoscopic cholecystectomy years before.'
      }).relevant
    ).toBe(false);
  });

  it('rejects loosely related search results', () => {
    expect(
      caseRelevance(topic, {
        meshDescriptorUis: ['D003967'],
        title: 'A rare case of neonatal-onset diarrhea due to congenital tufting enteropathy',
        text: 'Diarrhea in a neonate.'
      })
    ).toEqual({ relevant: false, reason: 'none' });
  });
});

describe('question frameworks', () => {
  it('are all drafts until a clinician approves them', () => {
    expect(Object.values(QUESTION_FRAMEWORKS).every((f) => f.status === 'draft')).toBe(true);
  });

  it('give procedures eight questions and nine ordered phases', () => {
    expect(PROCEDURE_FRAMEWORK.questions).toHaveLength(8);
    expect(PROCEDURE_FRAMEWORK.phases?.map((p) => p.number)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9]);
    expect(PROCEDURE_FRAMEWORK.phases?.find((p) => p.key === 'blood')?.quantitative).toBe(true);
  });

  it('use unique keys and non-empty search hints', () => {
    for (const framework of [PROCEDURE_FRAMEWORK, CONDITION_FRAMEWORK, MEDICATION_FRAMEWORK]) {
      const items = frameworkItems(framework);
      expect(new Set(items.map((i) => i.key)).size).toBe(items.length);
      expect(items.every((i) => i.searchHints.length > 0)).toBe(true);
    }
  });
});

describe('selectEvidence', () => {
  const candidate = (passageId: string, documentKey: string, text: string, sectionPath = 'Results', score = 0.5) => ({
    passageId,
    documentKey,
    title: 'Laparoscopic cholecystectomy outcomes',
    sectionPath,
    text,
    score
  });
  const options = { hints: ['bleeding', 'transfusion', 'blood'], topicTerms: ['laparoscopic cholecystectomy'], limit: 3 };

  it('cites at most one passage per document', () => {
    const selected = selectEvidence(
      [
        candidate('a1', 'doc-a', 'Blood loss and transfusion were low.'),
        candidate('a2', 'doc-a', 'Bleeding was rare.'),
        candidate('b1', 'doc-b', 'Bleeding occurred in two patients.')
      ],
      options
    );
    expect(selected.map((s) => s.passageId)).toEqual(['a1', 'b1']);
  });

  it('ranks passages covering more hints first', () => {
    const selected = selectEvidence(
      [
        candidate('few', 'doc-a', 'Bleeding was rare.', 'Results', 0.9),
        candidate('many', 'doc-b', 'Blood loss was low and no transfusion or bleeding occurred.', 'Results', 0.1)
      ],
      options
    );
    expect(selected[0].passageId).toBe('many');
  });

  it('requires the hint in the text, not only the section title', () => {
    expect(
      selectEvidence([candidate('x', 'doc-a', 'Patients were aged 18 to 65.', 'Blood management')], options)
    ).toEqual([]);
  });

  it('requires the document to concern the topic', () => {
    const offTopic = { ...candidate('x', 'doc-a', 'Blood transfusion in cardiac surgery.'), title: 'Cardiac surgery' };
    expect(selectEvidence([offTopic], options)).toEqual([]);
  });
});

describe('diagnosisGroup', () => {
  it('ignores demographic check tags', () => {
    expect(
      diagnosisGroup([
        { preferredTerm: 'Humans', treeNumbers: [] },
        { preferredTerm: 'Aged, 80 and over', treeNumbers: [] },
        { preferredTerm: 'Female', treeNumbers: [] },
        { preferredTerm: 'Cholelithiasis', treeNumbers: [] }
      ])
    ).toBe('Cholelithiasis');
  });

  it('uses only disease descriptors when tree numbers are known', () => {
    expect(
      diagnosisGroup([
        { preferredTerm: 'Gallbladder', treeNumbers: ['A03.620.400'] },
        { preferredTerm: 'Gallbladder Diseases', treeNumbers: ['C06.130'] },
        { preferredTerm: 'Situs Inversus', treeNumbers: ['C16.131.077.927'] }
      ])
    ).toBe('Gallbladder Diseases; Situs Inversus');
  });

  it('falls back to Unclassified', () => {
    expect(diagnosisGroup([{ preferredTerm: 'Humans', treeNumbers: [] }])).toBe('Unclassified');
  });
});

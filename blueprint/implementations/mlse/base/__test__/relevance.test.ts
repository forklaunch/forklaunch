import { FOLLOW_UP_QUESTIONS, suggestFollowUps } from '../domain/followUpQuestions';
import { detectTopicType, isOverviewQuery, OVERVIEW_SECTIONS, sectionForQuestion } from '../domain/overviewSections';
import { classifyQuery } from '../services/queryClassifier.service';
import { followUpQuery } from '../services/followUp.service';
import { clinicalTermsFor } from '../services/layTerms.service';
import { keySentences, passageIsAbout, queryConcepts } from '../services/relevance.service';

describe('clinicalTermsFor', () => {
  it('maps everyday terms, including run-together spellings', () => {
    expect(clinicalTermsFor('heart attack')).toEqual(['myocardial infarction', 'acute coronary syndrome']);
    expect(clinicalTermsFor('HEARTATTACK')).toEqual(['myocardial infarction', 'acute coronary syndrome']);
    expect(clinicalTermsFor('treatment of high blood pressure in pregnancy')).toEqual(['hypertension']);
    expect(clinicalTermsFor('bone marrow transplant')).toEqual(['hematopoietic stem cell transplantation']);
  });

  it('leaves clinical queries alone', () => {
    expect(clinicalTermsFor('myocardial infarction troponin')).toEqual([]);
    expect(clinicalTermsFor('laparoscopic cholecystectomy complications')).toEqual([]);
  });
});

describe('passageIsAbout', () => {
  // the two passages MLSE wrongly answered "HEARTATTACK" from in the demo
  const redditStatins = {
    title: 'Large Language Model–Based Analysis of Statin Therapy Discussions and Sentiment on Social Media',
    sectionPath: 'Results › Clinical Relevance Assessment',
    text: 'High-relevance content was most frequent in r/stroke (40/46, 87.0%), r/HeartAttack (41/87, 47.1%), r/diabetes_t2 (29/82, 35.4%).'
  };
  const gynaeAsystole = {
    title: 'Asystole Likely Due to a Vagal Reflex During Routine Gynecologic Surgery: A Case Report',
    sectionPath: 'Introduction',
    text: 'The patient experienced severe bradycardia that progressed to asystole after pneumoperitoneum during an otherwise routine laparoscopic gynecologic surgery.'
  };
  const infarctionTrial = {
    title: 'Early invasive strategy after acute myocardial infarction',
    sectionPath: 'Results',
    text: 'Among patients with acute myocardial infarction, mortality at 30 days was 4.1%.'
  };

  it('rejects passages that only look similar to the question', () => {
    const concepts = queryConcepts('HEARTATTACK', ['HEARTATTACK', 'myocardial infarction']);
    expect(passageIsAbout(redditStatins, concepts)).toBe(false);
    expect(passageIsAbout(gynaeAsystole, concepts)).toBe(false);
    expect(passageIsAbout(infarctionTrial, concepts)).toBe(true);
  });

  it('needs most of the question words when no clinical term is named', () => {
    const concepts = queryConcepts('bile leak after laparoscopic cholecystectomy', ['bile leak after laparoscopic cholecystectomy']);
    expect(
      passageIsAbout({ title: 'Outcomes', sectionPath: 'Complications', text: 'Bile leak after laparoscopic cholecystectomy occurred in 2% of patients.' }, concepts)
    ).toBe(true);
    expect(
      passageIsAbout({ title: 'Cefazolin', sectionPath: 'Dosage', text: 'Cefazolin is given before surgery.' }, concepts)
    ).toBe(false);
  });

  it('accepts a MeSH synonym the passage uses', () => {
    const concepts = queryConcepts('celioscopic cholecystectomy', ['celioscopic cholecystectomy', 'Cholecystectomy, Laparoscopic']);
    expect(
      passageIsAbout({ title: 'Outcomes of laparoscopic cholecystectomy', sectionPath: 'Results', text: 'Recovery was fast.' }, concepts)
    ).toBe(true);
  });

  it('ignores a passing mention and trial eligibility rules', () => {
    const concepts = queryConcepts('heart attack', ['heart attack', 'myocardial infarction']);
    expect(
      passageIsAbout({ title: 'AAA-SHAPE Pivotal Trial', sectionPath: 'Eligibility', text: 'Myocardial infarction within 3 months prior to the procedure. Myocardial infarction on imaging.' }, concepts)
    ).toBe(false);
    expect(
      passageIsAbout({ title: 'Aneurysm repair outcomes', sectionPath: 'Methods', text: 'Patients with a prior myocardial infarction were included. Sac size was measured.' }, concepts)
    ).toBe(false);
    expect(
      passageIsAbout({ title: 'Imaging', sectionPath: 'Results', text: 'Myocardial infarction caused remodelling. After myocardial infarction, scar size grew.' }, concepts)
    ).toBe(true);
  });
});

describe('keySentences', () => {
  it('quotes the sentence of each passage that best answers the question', () => {
    const concepts = queryConcepts('heart attack', ['heart attack', 'myocardial infarction']);
    const quotes = keySentences(
      [
        {
          passageId: 'a',
          text: 'Patients were enrolled at 12 centres. Among patients with acute myocardial infarction, mortality at 30 days was 4.1%. Follow-up lasted one year.'
        },
        { passageId: 'b', text: 'Statins were discussed on social media forums, including r/HeartAttack, by many users.' }
      ],
      concepts
    );
    expect(quotes).toEqual([
      { text: 'Among patients with acute myocardial infarction, mortality at 30 days was 4.1%.', passageId: 'a' }
    ]);
  });
});

describe('overview answers', () => {
  it('recognizes procedures, medicines and conditions', () => {
    expect(detectTopicType('laparoscopic cholecystectomy', [])).toBe('procedure');
    expect(detectTopicType('kidney dialysis', [])).toBe('procedure');
    expect(detectTopicType('cefazolin', [{ sourceKey: 'openfda', title: 'Cefazolin — WG Critical Care, LLC' }])).toBe('medication');
    // a label that only mentions the condition does not make it a drug
    expect(detectTopicType('heart attack', [{ sourceKey: 'openfda', title: 'Aspirin 81 mg' }])).toBe('condition');
  });

  it('gives short topic queries an overview and specific questions a direct answer', () => {
    expect(isOverviewQuery('heart attack', true)).toBe(true);
    expect(isOverviewQuery('high blood pressure', true)).toBe(true);
    expect(isOverviewQuery('cefazolin', false)).toBe(true);
    expect(isOverviewQuery('bile duct injury rate in laparoscopic cholecystectomy', false)).toBe(false);
  });

  it('defines sections with hints for every topic type', () => {
    for (const sections of Object.values(OVERVIEW_SECTIONS)) {
      expect(sections.length).toBeGreaterThanOrEqual(4);
      expect(sections.every((s) => s.searchHints.length > 0)).toBe(true);
    }
  });
});

describe('followUpQuery', () => {
  it('adds the topic to a follow-up that does not name it', () => {
    expect(followUpQuery('how is it treated?', 'heart attack')).toBe('heart attack: how is it treated?');
  });

  it('keeps a follow-up that already names the topic', () => {
    expect(followUpQuery('Is heart attack more common in women?', 'heart attack')).toBe('Is heart attack more common in women?');
  });
});

describe('sectionForQuestion', () => {
  it('matches a follow-up to the overview section it asks about', () => {
    expect(sectionForQuestion(OVERVIEW_SECTIONS.condition, 'How is it treated?')?.key).toBe('treatment');
    expect(sectionForQuestion(OVERVIEW_SECTIONS.condition, 'What are the warning signs?')?.key).toBe('presentation');
    expect(sectionForQuestion(OVERVIEW_SECTIONS.medication, 'What are the side effects?')?.key).toBe('adverse');
    expect(sectionForQuestion(OVERVIEW_SECTIONS.condition, 'Is it more common in women?')).toBeUndefined();
  });
});

describe('suggestFollowUps', () => {
  const passages = [
    { title: 'Heart Attack', sectionPath: 'What are the symptoms of a heart attack?', text: 'The most common symptoms include chest discomfort and shortness of breath.' },
    { title: 'Heart Attack', sectionPath: 'What is the treatment for a heart attack?', text: 'Treatments may include medicines and coronary angioplasty.' },
    { title: 'Prevention', sectionPath: 'Summary', text: 'Lifestyle changes help prevent a heart attack.' }
  ];

  it('suggests two answerable questions, uncovered parts first', () => {
    expect(
      suggestFollowUps({ topicType: 'condition', topic: 'heart attack', passages, asked: ['heart attack'], coveredSections: ['treatment'] })
    ).toEqual(['What are the warning signs of heart attack?', 'How can heart attack be prevented?']);
  });

  it('never suggests something the sources do not address', () => {
    const suggestions = suggestFollowUps({ topicType: 'condition', topic: 'heart attack', passages, asked: [], coveredSections: [] });
    expect(suggestions).not.toContain('How does heart attack differ in women?');
    expect(suggestions).not.toContain('What are the complications of heart attack?');
  });

  it('does not repeat questions already asked', () => {
    const suggestions = suggestFollowUps({
      topicType: 'condition',
      topic: 'heart attack',
      passages,
      asked: ['heart attack', 'What are the warning signs of heart attack?', 'How is it treated?'],
      coveredSections: []
    });
    expect(suggestions).toEqual(['How can heart attack be prevented?']);
  });

  it('only suggests literature questions', () => {
    for (const templates of Object.values(FOLLOW_UP_QUESTIONS)) {
      for (const t of templates) {
        expect(classifyQuery(t.template.replace('{topic}', 'propofol')).queryClass).toBe('literature_lookup');
      }
    }
  });
});

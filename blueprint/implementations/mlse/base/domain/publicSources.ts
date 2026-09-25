import { SourceDescriptorDto } from '@forklaunch/interfaces-mlse/types';

// Version 1's free sources: every one permits commercial use, which is why
// StatPearls (CC BY-NC-ND) and WHO guidelines (CC BY-NC-SA IGO) are absent.
// Terms were reviewed on 2026-09-23 and are re-checked per document by the
// license gate at ingestion, because a source's terms can change.
export const PUBLIC_SOURCES: readonly SourceDescriptorDto[] = [
  {
    id: 'openfda',
    name: 'openFDA drug labels',
    tier: 'regulatory',
    licenseTerms: 'Public domain (CC0)',
    commercialUse: true,
    liveQuery: true
  },
  {
    id: 'dailymed',
    name: 'DailyMed structured product labels',
    tier: 'regulatory',
    licenseTerms: 'US government work, public domain',
    commercialUse: true,
    liveQuery: false
  },
  {
    id: 'clinicaltrials',
    name: 'ClinicalTrials.gov',
    tier: 'trial_registry',
    licenseTerms: 'Public registry data',
    commercialUse: true,
    liveQuery: true
  },
  {
    id: 'mesh',
    name: 'Medical Subject Headings (MeSH)',
    tier: 'vocabulary',
    licenseTerms: 'Public domain (NLM)',
    commercialUse: true,
    liveQuery: false
  },
  {
    id: 'pubmed',
    name: 'PubMed',
    tier: 'literature_index',
    licenseTerms:
      'Citation metadata free to reuse; some abstracts are publisher-copyrighted, so abstracts are shown as short excerpts with links',
    commercialUse: true,
    liveQuery: true
  },
  {
    id: 'pmc_oa',
    name: 'PubMed Central Open Access Subset',
    tier: 'literature_full_text',
    licenseTerms:
      'Per-article Creative Commons license; only CC0, CC BY and CC BY-SA articles are stored in full',
    commercialUse: true,
    liveQuery: true
  }
];

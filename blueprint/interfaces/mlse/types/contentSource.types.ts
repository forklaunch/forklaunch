// The free, commercially usable sources MLSE ships with. Licensed sources a
// client adds later are identified by their own string ids.
export type PublicSourceId =
  | 'openfda'
  | 'dailymed'
  | 'clinicaltrials'
  | 'mesh'
  | 'pubmed'
  | 'pmc_oa';

// How authoritative a source is, used when ranking evidence.
export type SourceTier =
  | 'regulatory'
  | 'guideline'
  | 'trial_registry'
  | 'vocabulary'
  | 'literature_index'
  | 'literature_full_text'
  | 'licensed';

// What MLSE may store and show from a document, decided per document from its
// license before anything is written.
export type LicenseScope = 'full_text' | 'excerpt_only' | 'metadata_only';

export type SourceDescriptorDto = {
  id: string;
  name: string;
  tier: SourceTier;
  // human-readable license terms, e.g. 'Public domain (CC0)'
  licenseTerms: string;
  commercialUse: boolean;
  // whether the source is also queried live at search time, not only ingested
  liveQuery: boolean;
};

export type ContentSourceProviderParameters = {
  SourceDescriptorDto: SourceDescriptorDto;
};

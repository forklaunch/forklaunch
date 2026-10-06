// One titled part of a document, e.g. a drug label's "Dosage and
// Administration" or a paper's "Methods". Sections keep their path so every
// passage MLSE cites can say where in the document it came from.
export type DocumentSectionDto = {
  path: string;
  text: string;
};

// A document as a source returns it, before the license gate and chunking.
export type FetchedDocumentDto = {
  sourceKey: string;
  // the source's stable id: openFDA set_id, NCT number, PMID, PMCID, SetID
  externalId: string;
  title: string;
  url: string;
  // ISO date (YYYY-MM-DD) when the source provides one
  publishedAt?: string;
  // the license as the source states it; the license gate decides what may
  // be stored from it
  license?: string;
  isCaseReport?: boolean;
  retracted?: boolean;
  // MeSH descriptor UIs the source tagged the document with
  meshDescriptorUis?: string[];
  sections: DocumentSectionDto[];
};

export type SourceQueryDto = {
  term: string;
  limit: number;
};

export type MeshConceptDto = {
  descriptorUi: string;
  preferredTerm: string;
  synonyms: string[];
  treeNumbers: string[];
};

export type PassageDto = {
  sectionPath: string;
  ordinal: number;
  text: string;
};

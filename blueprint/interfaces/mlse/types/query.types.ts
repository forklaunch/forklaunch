// How a query is routed before any retrieval or generation happens. Only
// literature_lookup reaches the AI; the others get fixed, non-generated
// responses so the patient-data and emergency boundaries never depend on a
// model's judgment.
export type QueryClass =
  | 'literature_lookup'
  | 'patient_specific_treatment'
  | 'prescription_request'
  | 'exact_dosage_no_context'
  | 'emergency_pattern'
  | 'unverifiable_source_requested';

export type QueryClassificationDto = {
  queryClass: QueryClass;
  // the rule that decided it, recorded for audit
  reason: string;
  // PMIDs, PMC ids, NCT ids and DOIs named in the query; the answer service
  // checks they exist before answering from them
  sourceReferences: string[];
  // query words left after removing dosing words (the drug, for dosing
  // queries)
  subjectTerms: string[];
};

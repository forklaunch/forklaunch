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

// Fixed responses for queries that must not reach the AI. They are written
// once and reviewed, never generated, so what a doctor sees on these paths
// cannot vary between requests.
//
// STATUS: DRAFT. To be reviewed by a clinician and by legal (the patient-data
// and clinical-decision-support boundaries) before launch.

export const EMERGENCY_MESSAGE =
  'This looks like an emergency happening now. MLSE is a literature reference and does not give emergency guidance. Follow your hospital\'s emergency protocol or call your local emergency number; for a poisoning or overdose, contact your poison control centre.';

export const PATIENT_SPECIFIC_MESSAGE =
  'MLSE does not make treatment or dosing decisions for an individual patient. That needs clinical judgement and the full prescribing information. Where a drug label applies, its published dosing section is quoted below as written, without any calculation.';

export const PRESCRIPTION_MESSAGE =
  'MLSE cannot prescribe or issue prescriptions. It summarizes published literature and drug labels for clinicians.';

export const DOSAGE_NO_CONTEXT_MESSAGE =
  'The dose depends on the indication and the patient. Below is the published label dosing section, quoted as written. For a specific indication, search for the drug together with it (for example "cefazolin dose surgical prophylaxis").';

export const SOURCE_NOT_FOUND_MESSAGE =
  'MLSE does not hold the source named in this search, so it cannot say what that source reports. It answers only from sources it has retrieved and can cite.';

export const DRAFT_ANSWER_NOTICE =
  'AI-written from the cited sources and checked automatically: every sentence cites a source passage and every number appears in it. Not yet clinician-reviewed; not for clinical use.';

export const QUOTED_NOTICE =
  'Quoted from the cited sources without AI. Not for clinical use.';

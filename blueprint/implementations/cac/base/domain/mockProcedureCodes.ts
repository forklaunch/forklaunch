import { ProcedureCodeDto } from '@forklaunch/interfaces-cac/types';

//! Free, non-AMA placeholder procedure codes — carries the same shape/behavior
//! as real CPT without using AMA's actual code list or descriptions. See
//! plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §2 and §5.
export const MOCK_PROCEDURE_CODES: Record<string, ProcedureCodeDto> = {
  'PROC-001': { code: 'PROC-001', description: 'Office Visit' },
  'PROC-002': { code: 'PROC-002', description: 'Annual Physical Exam' },
  'PROC-003': { code: 'PROC-003', description: 'Diagnostic Lab Panel' },
  'PROC-004': { code: 'PROC-004', description: 'Blood Pressure Management Visit' },
  'PROC-005': { code: 'PROC-005', description: 'Diabetes Management Visit' },
  'PROC-006': { code: 'PROC-006', description: 'Behavioral Health Counseling' },
  'PROC-007': { code: 'PROC-007', description: 'Pulmonary Function Test' },
  'PROC-008': { code: 'PROC-008', description: 'Upper GI Endoscopy' },
  'PROC-009': { code: 'PROC-009', description: 'Urinalysis' },
  'PROC-010': { code: 'PROC-010', description: 'Wrist X-ray' },
  'PROC-011': { code: 'PROC-011', description: 'Leg X-ray' },
  // Recognized, but deliberately has no MOCK_LCD_CROSSWALK/NCCI entry —
  // the fixture used by tests that need a real, known code the other
  // three scrubbing layers have nothing to say about.
  'PROC-999': { code: 'PROC-999', description: 'Unmapped Procedure (test fixture)' }
};

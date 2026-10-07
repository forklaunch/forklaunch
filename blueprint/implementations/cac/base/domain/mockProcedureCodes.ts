import { ProcedureCodeDto } from '@forklaunch/interfaces-cac/types';

//! Free, non-AMA placeholder procedure codes — carries the same shape/behavior
//! as real CPT without using AMA's actual code list or descriptions. See
//! plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §2 and §5.
export const MOCK_PROCEDURE_CODES: Record<string, ProcedureCodeDto> = {
  'PROC-001': { code: 'PROC-001', description: 'Office Visit' },
  'PROC-002': { code: 'PROC-002', description: 'Annual Physical Exam' },
  'PROC-003': { code: 'PROC-003', description: 'Diagnostic Lab Panel' }
};

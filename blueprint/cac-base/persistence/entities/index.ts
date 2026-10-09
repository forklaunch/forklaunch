// Phase 1 (§4 of plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md). AuditLog
// isn't a local entity — it's handled by the framework's existing
// auditLogger.ts at the HTTP layer, not a MikroORM entity this module owns.
export { Charge } from './charge.entity';
export { Claim } from './claim.entity';
export { CodeSetLicense } from './codeSetLicense.entity';
export { CptCode } from './cptCode.entity';
export { Denial } from './denial.entity';
export { Diagnosis } from './diagnosis.entity';
export { Encounter } from './encounter.entity';
export { HcpcsCode } from './hcpcsCode.entity';
export { Icd10Code } from './icd10Code.entity';
export { Insurance } from './insurance.entity';
export { Patient } from './patient.entity';
export { Remittance } from './remittance.entity';

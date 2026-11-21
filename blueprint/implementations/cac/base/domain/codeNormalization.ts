// Diagnosis and procedure codes are compared in one canonical form: no dot,
// no whitespace, upper case. The CDC ICD-10-CM release omits the dot (E119)
// while clinicians and claims write it (E11.9); an exact-string comparison
// would treat them as different codes. The scrubbing rule tables are keyed
// in this form, and cac-base stores and looks up reference codes the same
// way, so every layer agrees on what "the same code" means.
export function normalizeCode(code: string): string {
  return code.replace(/[\s.]/g, '').toUpperCase();
}

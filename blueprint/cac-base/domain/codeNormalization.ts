// ICD-10-CM and HCPCS codes are stored and compared in one canonical form:
// no dot, no whitespace, upper case. The CDC release files omit the dot
// (E119) while clinicians and claims write it (E11.9), and an exact-string
// lookup would call one of them invalid.
export function normalizeCode(code: string): string {
  return code.replace(/[\s.]/g, '').toUpperCase();
}

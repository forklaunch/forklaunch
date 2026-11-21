// One definition, shared with the scrubbing rule tables in
// implementation-cac-base, so the reference tables, the claim path and the
// rules all normalize the same way: no dot, no whitespace, upper case
// (E11.9, e119 and " E119" all become E119).
export { normalizeCode } from '@forklaunch/implementation-cac-base/services';

// Tracks the *adopting organization's own* real-CPT connector/license status —
// never anything ForkLaunch itself holds. See
// plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md §2 and §5.
export const LicenseStatus = {
  NONE: 'none',
  PENDING: 'pending',
  ACTIVE: 'active'
} as const;
export type LicenseStatus = (typeof LicenseStatus)[keyof typeof LicenseStatus];

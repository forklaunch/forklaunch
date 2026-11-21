/**
 * A column index from an environment variable: unset means the fallback;
 * anything but a whole number 0 or more is an error, not NaN (which would
 * load nothing and still report success).
 */
export function columnIndexFromEnv(
  name: string,
  raw: string | undefined,
  fallback: number
): number {
  if (raw === undefined || raw.trim() === '') return fallback;
  const value = Number(raw);
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`${name} must be a column index (0 or more), got "${raw}"`);
  }
  return value;
}

/** "true"/"false" from an environment variable, or the fallback when unset. */
export function flagFromEnv(name: string, raw: string | undefined, fallback: boolean): boolean {
  if (raw === undefined || raw.trim() === '') return fallback;
  const value = raw.trim().toLowerCase();
  if (value === 'true') return true;
  if (value === 'false') return false;
  throw new Error(`${name} must be "true" or "false", got "${raw}"`);
}

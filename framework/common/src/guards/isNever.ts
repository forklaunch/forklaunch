/**
 * Type guard that checks if a value is of type never
 * @param value - The value to check
 * @returns Always returns true since this is a type guard for the never type
 */
export function isNever(_value: never): _value is never {
  return true;
}

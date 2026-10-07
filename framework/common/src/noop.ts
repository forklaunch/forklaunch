/**
 * A no-operation function that does nothing when called.
 * This is commonly used as a default or placeholder function
 * when a function parameter is optional but needs a default value.
 *
 * @example
 * ```ts
 * function withCallback(callback = noop) {
 *   // ... do something
 *   callback();
 * }
 * ```
 */
export function noop(..._args: unknown[]) {}

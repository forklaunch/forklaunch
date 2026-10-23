/** Structural creation-result validation only; no prior identity or ownership proof. */
export function validateProviderCreation(
  provider: unknown,
  object: string
): void {
  if (!provider || typeof provider !== 'object' || Array.isArray(provider)) {
    throw new Error('Invalid Stripe billing response');
  }
  const result = provider as Record<string, unknown>;
  validateProviderReference(result.id);
  if (result.object !== object) {
    throw new Error('Invalid Stripe billing response');
  }
}

/** Shape of a consumed reference only; no provider account or alias authority. */
export function validateProviderReference(value: unknown): void {
  if (
    typeof value !== 'string' ||
    !value.trim() ||
    Buffer.byteLength(value, 'utf8') > 1024 ||
    // Control characters are exactly what this check rejects.
    // eslint-disable-next-line no-control-regex
    /[\u0000-\u001f\u007f-\u009f]/u.test(value)
  ) {
    throw new Error('Invalid Stripe billing response');
  }
}

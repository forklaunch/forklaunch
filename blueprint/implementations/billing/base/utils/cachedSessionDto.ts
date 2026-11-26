// Redis cache values cross a JSON boundary. Restore only declared DTO dates;
// opaque provider payloads must retain their original representation.
export function cachedSessionDto<
  T extends { id?: string; expiresAt: Date; createdAt?: Date; updatedAt?: Date }
>(value: T, expectedId?: string): T {
  if (
    arguments.length > 1 &&
    (typeof expectedId !== 'string' ||
      !expectedId.trim() ||
      !value ||
      value.id !== expectedId)
  ) {
    throw new Error('Cached session identity does not match');
  }
  const date = (input: unknown): Date => {
    if (
      !(input instanceof Date) &&
      typeof input !== 'string' &&
      typeof input !== 'number'
    ) {
      throw new Error('Cached session date is invalid');
    }
    const parsed = new Date(input instanceof Date ? input.getTime() : input);
    if (!Number.isFinite(parsed.getTime())) {
      throw new Error('Cached session date is invalid');
    }
    return parsed;
  };
  return {
    ...value,
    expiresAt: date(value.expiresAt),
    ...(value.createdAt === undefined
      ? {}
      : { createdAt: date(value.createdAt) }),
    ...(value.updatedAt === undefined
      ? {}
      : { updatedAt: date(value.updatedAt) })
  };
}

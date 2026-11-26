/** An explicitly provider-owned HTTP surface, excluded from generated endpoint SDKs. */
export type DelegatedSurface = {
  readonly id: string;
  readonly provider: string;
  readonly coverage: 'provider-owned';
  readonly client: { readonly package: string; readonly export: string };
  readonly runtime: {
    readonly basePath: 'runtime-resolved';
    readonly openApi: boolean;
  };
  readonly callbacks: readonly string[];
};

/**
 * Declare the owner of dynamic provider routes while registering them normally.
 * This boundary does not certify provider contracts or execute them at build time.
 * The provider's own client remains authoritative for these endpoints.
 */
export function registerDelegatedSurface<T>(
  surface: DelegatedSurface,
  mount: { basePath: string; paths: string | string[] },
  register: (mount: { basePath: string; paths: string | string[] }) => T
): T {
  if (
    !surface.id ||
    !surface.provider ||
    surface.coverage !== 'provider-owned' ||
    !surface.client.package ||
    !surface.client.export ||
    surface.runtime.basePath !== 'runtime-resolved' ||
    typeof surface.runtime.openApi !== 'boolean' ||
    !Array.isArray(surface.callbacks) ||
    surface.callbacks.some(
      (callback) => typeof callback !== 'string' || !callback.startsWith('/')
    )
  ) {
    throw new Error('Invalid delegated HTTP surface descriptor');
  }
  const paths = Array.isArray(mount.paths) ? mount.paths : [mount.paths];
  if (
    (mount.basePath !== '' && !mount.basePath.startsWith('/')) ||
    paths.length === 0 ||
    paths.some(
      (path) =>
        !path.startsWith('/') ||
        (mount.basePath !== '' && !path.startsWith(mount.basePath + '/'))
    )
  ) {
    throw new Error('Invalid delegated HTTP surface mount');
  }
  return register(mount);
}

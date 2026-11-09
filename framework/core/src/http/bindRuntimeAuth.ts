/** Portable policy shared by contract compilation and runtime authentication. */
export type PortableJwtPolicy = {
  readonly scheme: 'jwt';
  readonly headerName?: string;
  readonly tokenPrefix?: string;
  readonly sessionSchema?: Readonly<Record<string, unknown>>;
} & (
  | { readonly allowedRoles: readonly string[] }
  | { readonly allowedPermissions: readonly string[] }
);

export type PortableHmacPolicy = {
  readonly scheme: 'hmac';
  readonly headerName?: string;
  readonly tokenPrefix?: string;
};

export type PortableAuthPolicy = PortableJwtPolicy | PortableHmacPolicy;

/** Runtime keys and resource decoding belong to DI, never portable SDK assets. */
export type JwtRuntimeBinding = (
  { readonly jwksPublicKeyUrl: string } | { readonly signatureKey: string }
) & {
  readonly decodeResource?: (
    token: string
  ) => Record<string, unknown> | Promise<Record<string, unknown>>;
};
export type HmacRuntimeBinding = {
  readonly secretKeys: Readonly<Record<string, string>>;
};

type RuntimeBinding<P extends PortableAuthPolicy> = P extends PortableJwtPolicy
  ? JwtRuntimeBinding
  : HmacRuntimeBinding;

type BoundPolicy<P> = Omit<
  P,
  'scheme' | 'allowedRoles' | 'allowedPermissions'
> &
  (P extends { readonly allowedRoles: readonly string[] }
    ? { allowedRoles: Set<string> }
    : {}) &
  (P extends { readonly allowedPermissions: readonly string[] }
    ? { allowedPermissions: Set<string> }
    : {});

type BoundContract<T extends { readonly auth: PortableAuthPolicy }, B> = Omit<
  T,
  'auth'
> & {
  auth: BoundPolicy<T['auth']> &
    (T['auth'] extends PortableJwtPolicy
      ? { jwt: Omit<B, 'decodeResource'> } & Pick<
          B,
          Extract<keyof B, 'decodeResource'>
        >
      : { hmac: B });
};

/** Bind credentials without allowing runtime configuration to override policy. */
export function bindRuntimeAuth<
  T extends { readonly auth: PortableAuthPolicy },
  B extends RuntimeBinding<T['auth']>
>(details: T, binding: B): BoundContract<T, B>;
export function bindRuntimeAuth(
  details: { readonly auth: PortableAuthPolicy },
  binding: JwtRuntimeBinding | HmacRuntimeBinding
): unknown {
  const policy: Record<string, unknown> = { ...details.auth };
  const keys = Object.keys(binding);
  const credentials: Record<string, unknown> = { ...binding };
  const invalid = (): never => {
    throw new Error('Invalid runtime authentication binding');
  };
  const allowedPolicy =
    policy.scheme === 'jwt'
      ? [
          'scheme',
          'headerName',
          'tokenPrefix',
          'sessionSchema',
          'allowedRoles',
          'allowedPermissions'
        ]
      : ['scheme', 'headerName', 'tokenPrefix'];
  if (Object.keys(policy).some((key) => !allowedPolicy.includes(key)))
    invalid();
  for (const key of ['headerName', 'tokenPrefix']) {
    if (
      policy[key] !== undefined &&
      (typeof policy[key] !== 'string' || !policy[key])
    )
      invalid();
  }
  if (
    policy.scheme === 'jwt' &&
    policy.allowedRoles === undefined &&
    policy.allowedPermissions === undefined
  )
    invalid();
  let authentication: Record<string, unknown>;
  if (policy.scheme === 'jwt') {
    if (
      keys.some(
        (key) =>
          !['jwksPublicKeyUrl', 'signatureKey', 'decodeResource'].includes(key)
      )
    )
      invalid();
    const keyFields = keys.filter((key) => key !== 'decodeResource');
    if (
      keyFields.length !== 1 ||
      typeof credentials[keyFields[0]] !== 'string' ||
      !credentials[keyFields[0]]
    )
      invalid();
    const decodeResource = credentials.decodeResource;
    if (decodeResource !== undefined && typeof decodeResource !== 'function')
      invalid();
    delete credentials.decodeResource;
    authentication = {
      jwt: credentials,
      ...(decodeResource === undefined ? {} : { decodeResource })
    };
  } else if (policy.scheme === 'hmac') {
    if (
      keys.length !== 1 ||
      keys[0] !== 'secretKeys' ||
      !credentials.secretKeys ||
      typeof credentials.secretKeys !== 'object' ||
      Array.isArray(credentials.secretKeys) ||
      Object.keys(credentials.secretKeys).length === 0 ||
      Object.values(credentials.secretKeys).some(
        (key) => typeof key !== 'string' || !key
      )
    )
      invalid();
    authentication = { hmac: credentials };
  } else {
    return invalid();
  }
  delete policy.scheme;
  for (const key of ['allowedRoles', 'allowedPermissions']) {
    if (policy[key] !== undefined) {
      const values = policy[key];
      if (
        !Array.isArray(values) ||
        values.some((value) => typeof value !== 'string')
      )
        invalid();
      policy[key] = new Set(values as string[]);
    }
  }
  return { ...details, auth: { ...policy, ...authentication } };
}

import type { JWK } from 'jose';

/**
 * JWT signature algorithm allow-lists.
 *
 * The framework never lets a token choose how it is verified: every
 * `jwtVerify` call is given an explicit list. The list comes from the kind of
 * key the app configured, narrowed by the app's own `algorithms` setting:
 *
 * - a shared secret (`signatureKey`) verifies HMAC algorithms only, `HS256`
 *   unless the app lists others;
 * - a public key (`jwksPublicKey` / `jwksPublicKeyUrl`) verifies only the
 *   asymmetric algorithms that fit that key. HMAC algorithms are refused
 *   there whatever the configuration says, which is what stops a forged
 *   token "signed" with the public key as an HMAC secret;
 * - `none` is refused everywhere.
 */

/** HMAC algorithms a shared secret may verify. */
export const SHARED_SECRET_JWT_ALGORITHMS: readonly string[] = [
  'HS256',
  'HS384',
  'HS512'
];

/** What a shared secret verifies when the app does not say. */
export const DEFAULT_SHARED_SECRET_JWT_ALGORITHMS: readonly string[] = [
  'HS256'
];

const RSA_JWT_ALGORITHMS: readonly string[] = [
  'RS256',
  'RS384',
  'RS512',
  'PS256',
  'PS384',
  'PS512'
];

const EC_CURVE_JWT_ALGORITHMS: Readonly<Record<string, readonly string[]>> = {
  'P-256': ['ES256'],
  'P-384': ['ES384'],
  'P-521': ['ES512']
};

const OKP_CURVE_JWT_ALGORITHMS: Readonly<Record<string, readonly string[]>> = {
  Ed25519: ['EdDSA', 'Ed25519']
};

function isSymmetricOrUnsigned(algorithm: string): boolean {
  const normalized = algorithm.toUpperCase();
  return normalized === 'NONE' || normalized.startsWith('HS');
}

function configuredList(
  configured: readonly string[] | undefined
): string[] | undefined {
  if (configured == null) return undefined;
  if (!Array.isArray(configured)) {
    throw new Error('JWT `algorithms` must be an array of algorithm names');
  }
  return configured.filter(
    (algorithm): algorithm is string => typeof algorithm === 'string'
  );
}

/**
 * The algorithms accepted for a shared secret: the configured HMAC
 * algorithms, else `HS256`. Anything that is not `HS256`/`HS384`/`HS512`
 * (including `none`) is dropped; an empty result is an error, never "any".
 */
export function jwtAlgorithmsForSharedSecret(
  configured?: readonly string[]
): string[] {
  const requested = configuredList(configured);
  if (requested === undefined) {
    return [...DEFAULT_SHARED_SECRET_JWT_ALGORITHMS];
  }
  const allowed = SHARED_SECRET_JWT_ALGORITHMS.filter((algorithm) =>
    requested.includes(algorithm)
  );
  if (allowed.length === 0) {
    throw new Error(
      'No usable JWT algorithm: a `signatureKey` verifies only HS256, HS384 or HS512'
    );
  }
  return allowed;
}

/** The asymmetric algorithms a public JWK can verify, from its type. */
function algorithmsFittingPublicKey(key: JWK): readonly string[] {
  let fitting: readonly string[];
  switch (key.kty) {
    case 'RSA':
      fitting = RSA_JWT_ALGORITHMS;
      break;
    case 'EC':
      fitting = EC_CURVE_JWT_ALGORITHMS[key.crv ?? ''] ?? [];
      break;
    case 'OKP':
      fitting = OKP_CURVE_JWT_ALGORITHMS[key.crv ?? ''] ?? [];
      break;
    case 'AKP':
      // Post-quantum signature keys always name their one algorithm.
      fitting = typeof key.alg === 'string' ? [key.alg] : [];
      break;
    default:
      // `oct` (a symmetric secret) and unknown key types verify nothing here.
      fitting = [];
  }
  if (typeof key.alg === 'string') {
    // A key that declares its algorithm is used for that algorithm only.
    fitting = fitting.filter((algorithm) => algorithm === key.alg);
  }
  return fitting.filter((algorithm) => !isSymmetricOrUnsigned(algorithm));
}

/**
 * The algorithms accepted for one public key: those that fit the key,
 * narrowed to the configured list when there is one. Never `none`, never an
 * HMAC algorithm. An empty result is an error, never "any".
 */
export function jwtAlgorithmsForPublicKey(
  key: JWK,
  configured?: readonly string[]
): string[] {
  if (key == null || typeof key !== 'object') {
    throw new Error('No usable JWT algorithm: the verification key is missing');
  }
  if (key.kty === 'oct') {
    throw new Error(
      'No usable JWT algorithm: a symmetric (oct) key cannot be used as a public key; use `signatureKey` for a shared secret'
    );
  }
  const requested = configuredList(configured);
  const allowed = algorithmsFittingPublicKey(key).filter(
    (algorithm) => requested === undefined || requested.includes(algorithm)
  );
  if (allowed.length === 0) {
    throw new Error(
      'No usable JWT algorithm: none of the permitted asymmetric algorithms fits this public key'
    );
  }
  return allowed;
}

function claimExpectation(
  name: 'issuer' | 'audience',
  value: string | readonly string[] | undefined
): string | string[] | undefined {
  if (value === undefined) return undefined;
  const values = typeof value === 'string' ? [value] : value;
  if (
    !Array.isArray(values) ||
    values.length === 0 ||
    values.some((entry) => typeof entry !== 'string' || entry.length === 0)
  ) {
    // An empty value (an unset environment variable, say) must not turn the
    // check off silently.
    throw new Error(
      `JWT \`${name}\` is configured but empty; set it to the expected value or remove it`
    );
  }
  return typeof value === 'string' ? value : [...values];
}

/**
 * The `issuer` / `audience` checks to hand to jose: present only when the
 * app configured them, so tokens are checked exactly as before otherwise.
 */
export function jwtClaimChecks(config: {
  readonly issuer?: string | readonly string[];
  readonly audience?: string | readonly string[];
}): { issuer?: string | string[]; audience?: string | string[] } {
  const issuer = claimExpectation('issuer', config.issuer);
  const audience = claimExpectation('audience', config.audience);
  return {
    ...(issuer === undefined ? {} : { issuer }),
    ...(audience === undefined ? {} : { audience })
  };
}

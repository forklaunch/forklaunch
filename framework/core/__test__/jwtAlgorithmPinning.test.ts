import { createHmac } from 'crypto';
import {
  exportJWK,
  exportSPKI,
  generateKeyPair,
  JWK,
  JWTPayload,
  SignJWT
} from 'jose';
import { beforeAll, describe, expect, it } from 'vitest';
import { discriminateAuthMethod } from '../src/http/discriminateAuthMethod';
import {
  jwtAlgorithmsForPublicKey,
  jwtAlgorithmsForSharedSecret,
  jwtClaimChecks
} from '../src/http/jwtAlgorithms';

type JwtConfig = Record<string, unknown>;

/** The framework's own verifier for a `jwt` auth configuration. */
async function verifierFor(
  jwt: JwtConfig
): Promise<(token: string) => Promise<JWTPayload | undefined>> {
  const { type, auth } = await discriminateAuthMethod({ jwt } as never);
  if (type !== 'jwt' || !('verificationFunction' in auth)) {
    throw new Error('expected a jwt verifier');
  }
  return auth.verificationFunction;
}

const base64url = (value: string | Buffer) =>
  Buffer.from(value).toString('base64url');

/** A hand-built compact JWT, for algorithms a signing library refuses. */
function forgeToken(
  header: Record<string, unknown>,
  payload: Record<string, unknown>,
  sign?: (signingInput: string) => Buffer
): string {
  const signingInput = `${base64url(JSON.stringify(header))}.${base64url(
    JSON.stringify(payload)
  )}`;
  return `${signingInput}.${sign ? base64url(sign(signingInput)) : ''}`;
}

const claims = () => ({
  sub: 'user-1',
  exp: Math.floor(Date.now() / 1000) + 3600
});

describe('JWT algorithm pinning', () => {
  // A private JWK, so one key can sign with every RSA algorithm.
  let rsaPrivate: JWK;
  let rsaJwk: JWK;
  let rsaPublicPem: string;
  let ecPrivate: CryptoKey;
  let ecJwk: JWK;

  beforeAll(async () => {
    const rsa = await generateKeyPair('RS256', { extractable: true });
    rsaPrivate = await exportJWK(rsa.privateKey);
    rsaJwk = await exportJWK(rsa.publicKey);
    rsaPublicPem = await exportSPKI(rsa.publicKey);
    const ec = await generateKeyPair('ES256', { extractable: true });
    ecPrivate = ec.privateKey;
    ecJwk = await exportJWK(ec.publicKey);
  });

  const signRsa = (alg: string, payload: Record<string, unknown> = {}) =>
    new SignJWT({ sub: 'user-1', ...payload })
      .setProtectedHeader({ alg })
      .setExpirationTime('1h')
      .sign(rsaPrivate);

  const signShared = (
    alg: string,
    secret: string,
    payload: Record<string, unknown> = {}
  ) =>
    new SignJWT({ sub: 'user-1', ...payload })
      .setProtectedHeader({ alg })
      .setExpirationTime('1h')
      .sign(new Uint8Array(Buffer.from(secret)));

  describe('public key / JWKS', () => {
    it('accepts a token signed with the matching asymmetric algorithm', async () => {
      const verify = await verifierFor({ jwksPublicKey: rsaJwk });
      expect((await verify(await signRsa('RS256')))?.sub).toBe('user-1');
      // Any algorithm that fits the key type is allowed by default.
      expect((await verify(await signRsa('PS256')))?.sub).toBe('user-1');
    });

    it('rejects an HS256 token signed with the public key as the secret (algorithm confusion)', async () => {
      const verify = await verifierFor({ jwksPublicKey: rsaJwk });
      for (const secret of [rsaPublicPem, JSON.stringify(rsaJwk)]) {
        const forged = forgeToken(
          { alg: 'HS256', typ: 'JWT' },
          claims(),
          (input) => createHmac('sha256', secret).update(input).digest()
        );
        await expect(verify(forged)).rejects.toMatchObject({
          code: 'ERR_JOSE_ALG_NOT_ALLOWED'
        });
      }
    });

    it('still rejects HS256 when the app lists it for a public key', async () => {
      const verify = await verifierFor({
        jwksPublicKey: rsaJwk,
        algorithms: ['HS256', 'RS256']
      });
      const forged = forgeToken({ alg: 'HS256' }, claims(), (input) =>
        createHmac('sha256', rsaPublicPem).update(input).digest()
      );
      await expect(verify(forged)).rejects.toMatchObject({
        code: 'ERR_JOSE_ALG_NOT_ALLOWED'
      });
      expect((await verify(await signRsa('RS256')))?.sub).toBe('user-1');
    });

    it('rejects an unsigned token (alg none)', async () => {
      const verify = await verifierFor({ jwksPublicKey: rsaJwk });
      for (const alg of ['none', 'None', 'NONE']) {
        await expect(
          verify(forgeToken({ alg }, claims()))
        ).rejects.toBeDefined();
      }
    });

    it('rejects a token signed with an algorithm outside the configured allow-list', async () => {
      const verify = await verifierFor({
        jwksPublicKey: rsaJwk,
        algorithms: ['RS256']
      });
      await expect(verify(await signRsa('RS384'))).rejects.toMatchObject({
        code: 'ERR_JOSE_ALG_NOT_ALLOWED'
      });
      await expect(verify(await signRsa('PS256'))).rejects.toMatchObject({
        code: 'ERR_JOSE_ALG_NOT_ALLOWED'
      });
      expect((await verify(await signRsa('RS256')))?.sub).toBe('user-1');
    });

    it('uses only the algorithm a key declares', async () => {
      const verify = await verifierFor({
        jwksPublicKey: { ...rsaJwk, alg: 'RS256' }
      });
      await expect(verify(await signRsa('RS512'))).rejects.toMatchObject({
        code: 'ERR_JOSE_ALG_NOT_ALLOWED'
      });
      expect((await verify(await signRsa('RS256')))?.sub).toBe('user-1');
    });

    it('pins each key of a JWKS to its own key type', async () => {
      const jwksPublicKeyUrl = `data:application/json,${encodeURIComponent(
        JSON.stringify({
          keys: [
            { ...rsaJwk, kid: 'rsa' },
            { ...ecJwk, kid: 'ec' }
          ]
        })
      )}`;
      const verify = await verifierFor({ jwksPublicKeyUrl });
      const ecToken = await new SignJWT({ sub: 'ec-user' })
        .setProtectedHeader({ alg: 'ES256', kid: 'ec' })
        .setExpirationTime('1h')
        .sign(ecPrivate);
      expect((await verify(ecToken))?.sub).toBe('ec-user');
      const forged = forgeToken({ alg: 'HS256' }, claims(), (input) =>
        createHmac('sha256', rsaPublicPem).update(input).digest()
      );
      await expect(verify(forged)).rejects.toBeDefined();
    });

    it('refuses a symmetric key in the public key slot', async () => {
      const secret = 'a-shared-secret-of-reasonable-length';
      const verify = await verifierFor({
        jwksPublicKey: { kty: 'oct', k: base64url(secret) }
      });
      await expect(verify(await signShared('HS256', secret))).rejects.toThrow(
        /symmetric/
      );
    });
  });

  describe('shared secret (signatureKey)', () => {
    const secret = 'a-shared-secret-of-at-least-64-bytes-so-every-hmac-size-accepts-it!!';

    it('accepts HS256 by default', async () => {
      const verify = await verifierFor({ signatureKey: secret });
      expect((await verify(await signShared('HS256', secret)))?.sub).toBe(
        'user-1'
      );
    });

    it('rejects other HMAC algorithms unless configured', async () => {
      const verify = await verifierFor({ signatureKey: secret });
      await expect(
        verify(await signShared('HS512', secret))
      ).rejects.toMatchObject({ code: 'ERR_JOSE_ALG_NOT_ALLOWED' });

      const configured = await verifierFor({
        signatureKey: secret,
        algorithms: ['HS512']
      });
      expect((await configured(await signShared('HS512', secret)))?.sub).toBe(
        'user-1'
      );
      await expect(
        configured(await signShared('HS256', secret))
      ).rejects.toMatchObject({ code: 'ERR_JOSE_ALG_NOT_ALLOWED' });
    });

    it('rejects an unsigned token and a wrong secret', async () => {
      const verify = await verifierFor({ signatureKey: secret });
      await expect(
        verify(forgeToken({ alg: 'none' }, claims()))
      ).rejects.toBeDefined();
      await expect(
        verify(await signShared('HS256', `${secret}x`))
      ).rejects.toBeDefined();
    });

    it('never treats `none` or an asymmetric algorithm as allowed', async () => {
      const verify = await verifierFor({
        signatureKey: secret,
        algorithms: ['none']
      });
      await expect(
        verify(forgeToken({ alg: 'none' }, claims()))
      ).rejects.toThrow(/No usable JWT algorithm/);
      await expect(verify(await signShared('HS256', secret))).rejects.toThrow(
        /No usable JWT algorithm/
      );
    });
  });

  describe('issuer and audience', () => {
    const secret = 'a-shared-secret-of-reasonable-length';

    it('are not checked unless configured', async () => {
      const verify = await verifierFor({ signatureKey: secret });
      const token = await signShared('HS256', secret, {
        iss: 'anyone',
        aud: 'anything'
      });
      expect((await verify(token))?.iss).toBe('anyone');
    });

    it('are enforced when configured', async () => {
      const verify = await verifierFor({
        signatureKey: secret,
        issuer: 'https://id.example.com',
        audience: ['api', 'admin-api']
      });
      const good = await signShared('HS256', secret, {
        iss: 'https://id.example.com',
        aud: 'api'
      });
      expect((await verify(good))?.sub).toBe('user-1');
      await expect(
        verify(
          await signShared('HS256', secret, {
            iss: 'https://evil.example.com',
            aud: 'api'
          })
        )
      ).rejects.toMatchObject({ code: 'ERR_JWT_CLAIM_VALIDATION_FAILED' });
      await expect(
        verify(
          await signShared('HS256', secret, {
            iss: 'https://id.example.com',
            aud: 'other-service'
          })
        )
      ).rejects.toMatchObject({ code: 'ERR_JWT_CLAIM_VALIDATION_FAILED' });
      await expect(
        verify(await signShared('HS256', secret))
      ).rejects.toMatchObject({ code: 'ERR_JWT_CLAIM_VALIDATION_FAILED' });
    });

    it('an empty configured value rejects instead of switching the check off', async () => {
      const verify = await verifierFor({ signatureKey: secret, issuer: '' });
      await expect(
        verify(await signShared('HS256', secret, { iss: 'anyone' }))
      ).rejects.toThrow(/issuer/);
      expect(() => jwtClaimChecks({ audience: [] })).toThrow(/audience/);
    });
  });

  describe('allow-list helpers', () => {
    it('shared secret: HS256 by default, HMAC algorithms only', () => {
      expect(jwtAlgorithmsForSharedSecret()).toEqual(['HS256']);
      expect(jwtAlgorithmsForSharedSecret(['HS384', 'RS256', 'none'])).toEqual(
        ['HS384']
      );
      expect(() => jwtAlgorithmsForSharedSecret([])).toThrow();
      expect(() => jwtAlgorithmsForSharedSecret(['RS256'])).toThrow();
    });

    it('public key: asymmetric algorithms that fit the key, never HS* or none', () => {
      expect(jwtAlgorithmsForPublicKey({ kty: 'RSA' })).toEqual([
        'RS256',
        'RS384',
        'RS512',
        'PS256',
        'PS384',
        'PS512'
      ]);
      expect(jwtAlgorithmsForPublicKey({ kty: 'EC', crv: 'P-256' })).toEqual([
        'ES256'
      ]);
      expect(jwtAlgorithmsForPublicKey({ kty: 'EC', crv: 'P-384' })).toEqual([
        'ES384'
      ]);
      expect(
        jwtAlgorithmsForPublicKey({ kty: 'OKP', crv: 'Ed25519' })
      ).toEqual(['EdDSA', 'Ed25519']);
      expect(
        jwtAlgorithmsForPublicKey({ kty: 'RSA' }, ['HS256', 'none', 'PS256'])
      ).toEqual(['PS256']);
      expect(() => jwtAlgorithmsForPublicKey({ kty: 'RSA' }, ['HS256'])).toThrow();
      expect(() => jwtAlgorithmsForPublicKey({ kty: 'RSA' }, [])).toThrow();
      expect(() => jwtAlgorithmsForPublicKey({ kty: 'RSA', alg: 'HS256' })).toThrow();
      expect(() => jwtAlgorithmsForPublicKey({ kty: 'RSA', alg: 'none' })).toThrow();
      expect(() => jwtAlgorithmsForPublicKey({ kty: 'EC', crv: 'P-256' }, ['RS256'])).toThrow();
      expect(() => jwtAlgorithmsForPublicKey({ kty: 'oct', k: 'c2VjcmV0' })).toThrow();
      expect(() => jwtAlgorithmsForPublicKey({ kty: 'unknown' })).toThrow();
    });
  });
});

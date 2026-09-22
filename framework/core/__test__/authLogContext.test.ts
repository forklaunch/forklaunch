import { SignJWT, errors as joseErrors } from 'jose';
import { describe, expect, it } from 'vitest';
import {
  authLogContext,
  jwtFailureReason
} from '../src/http/middleware/request/authLogContext';

async function makeJwt(claims: Record<string, unknown>) {
  const secret = new TextEncoder().encode(
    'test-secret-test-secret-test-secret'
  );
  return new SignJWT(claims)
    .setProtectedHeader({ alg: 'HS256', kid: 'k1' })
    .setIssuer('https://iam.example')
    .setExpirationTime('1h')
    .sign(secret);
}

describe('authLogContext', () => {
  it('never includes the credential, only a fingerprint and claims', async () => {
    const jwt = await makeJwt({
      sub: 'user-1',
      organizationId: 'org-1',
      email: 'a@b.c'
    });
    const header = `Bearer ${jwt}`;
    const ctx = authLogContext(header);

    expect(JSON.stringify(ctx)).not.toContain(jwt);
    expect(JSON.stringify(ctx)).not.toContain('a@b.c');
    expect(ctx.hasToken).toBe(true);
    expect(ctx.tokenFingerprint).toMatch(/^[0-9a-f]{8}$/);
    expect(ctx.claimed).toMatchObject({
      sub: 'user-1',
      organizationId: 'org-1',
      iss: 'https://iam.example',
      kid: 'k1'
    });
    expect(typeof ctx.claimed?.exp).toBe('number');
  });

  it('gives the same fingerprint for the same token and different for another', async () => {
    const a = `Bearer ${await makeJwt({ sub: 'a' })}`;
    const b = `Bearer ${await makeJwt({ sub: 'b' })}`;
    expect(authLogContext(a).tokenFingerprint).toBe(
      authLogContext(a).tokenFingerprint
    );
    expect(authLogContext(a).tokenFingerprint).not.toBe(
      authLogContext(b).tokenFingerprint
    );
  });

  it('reports a missing header without a fingerprint', () => {
    expect(authLogContext(undefined)).toEqual({ hasToken: false });
    expect(authLogContext('')).toEqual({ hasToken: false });
  });

  it('does not decode a Basic credential and does not leak it', () => {
    const header = `Basic ${Buffer.from('alice:hunter2').toString('base64')}`;
    const ctx = authLogContext(header);
    expect(ctx.claimed).toBeUndefined();
    expect(JSON.stringify(ctx)).not.toContain('hunter2');
    expect(JSON.stringify(ctx)).not.toContain(header.split(' ')[1]);
  });

  it('tolerates something that only looks like a JWT', () => {
    const ctx = authLogContext('Bearer not.a.jwt');
    expect(ctx.hasToken).toBe(true);
    expect(ctx.claimed).toBeUndefined();
  });
});

describe('jwtFailureReason', () => {
  it('names expiry, bad signature and malformed tokens distinctly', () => {
    expect(jwtFailureReason(new joseErrors.JWTExpired('x', {}))).toBe(
      'jwt_expired'
    );
    expect(
      jwtFailureReason(new joseErrors.JWSSignatureVerificationFailed())
    ).toBe('jwt_bad_signature');
    expect(jwtFailureReason(new joseErrors.JWTInvalid('x'))).toBe(
      'jwt_malformed'
    );
    expect(jwtFailureReason(new Error('boom'))).toBe('unknown');
  });
});

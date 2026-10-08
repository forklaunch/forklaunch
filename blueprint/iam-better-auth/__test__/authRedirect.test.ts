import { describe, expect, it } from 'vitest';
import {
  configuredAuthOrigin,
  inlineScriptJson,
  trustedAuthCallback
} from '../domain/utils/authRedirect.util';

describe('token-bearing login redirects', () => {
  it.each([
    'https://evil.example/callback',
    'https://app.example.evil.example',
    'https://app.example:8443',
    '//app.example/callback',
    'javascript:alert(1)',
    'https://user:pass@app.example',
    'https://app.example/callback#token',
    'https://app.example\\@evil.example',
    ' https://app.example/callback'
  ])('rejects untrusted or ambiguous destination %s', (value) => {
    expect(trustedAuthCallback(value, ['https://app.example'])).toBeNull();
  });
  it('accepts an exact configured browser origin and preserves state', () => {
    expect(
      trustedAuthCallback('https://app.example/callback?state=abc', [
        'https://app.example'
      ])?.toString()
    ).toBe('https://app.example/callback?state=abc');
  });
  it('does not treat a wildcard or an array query parameter as a token destination', () => {
    expect(
      trustedAuthCallback('https://app.example', ['*', 'https://*.example'])
    ).toBeNull();
    expect(
      trustedAuthCallback(['https://app.example'], ['https://app.example'])
    ).toBeNull();
  });
  it('uses only a configured HTTP origin for internal authentication requests', () => {
    expect(configuredAuthOrigin('https://iam.example/api/auth')).toBe(
      'https://iam.example'
    );
    for (const value of [
      undefined,
      'javascript:alert(1)',
      'https://user:password@iam.example'
    ])
      expect(() => configuredAuthOrigin(value)).toThrow();
  });
  it('keeps hostile script text inside JSON without changing the decoded value', () => {
    const value = { provider: '</script><script>bad()</script>&\u2028' };
    const encoded = inlineScriptJson(value);
    expect(encoded).not.toContain('<');
    expect(JSON.parse(encoded)).toEqual(value);
  });
});

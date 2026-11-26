import { describe, expect, it } from 'vitest';
import {
  EMAIL_LIMITS,
  type EmailListUnsubscribe,
  EmailRequestError,
  validateEmailSendRequest,
  validateListUnsubscribe
} from '../src/http/emailClient';

/**
 * `listUnsubscribe` is checked before the call, with the gateway's rules:
 * https only, a plain mailbox, one-click only with a URL, and nothing that
 * could end a header line or add another target.
 */

const URL_OK = 'https://members.example.com/unsubscribe?token=abc%2C123';

function refusal(value: unknown): EmailRequestError {
  try {
    validateListUnsubscribe(value);
  } catch (error) {
    expect(error).toBeInstanceOf(EmailRequestError);
    if (error instanceof EmailRequestError) {
      expect(error.status).toBe(400);
      return error;
    }
  }
  throw new Error(`accepted ${JSON.stringify(value)}`);
}

describe('validateListUnsubscribe', () => {
  it('accepts a url, a mailto, or both, and absence', () => {
    expect(validateListUnsubscribe(undefined)).toBeUndefined();
    expect(validateListUnsubscribe({ url: URL_OK, oneClick: true })).toEqual({
      url: URL_OK,
      oneClick: true
    });
    expect(
      validateListUnsubscribe({ mailto: 'unsubscribe@example.com' })
    ).toEqual({ mailto: 'unsubscribe@example.com' });
    expect(
      validateListUnsubscribe({
        url: URL_OK,
        mailto: 'u+list@example.com',
        oneClick: false
      })
    ).toEqual({ url: URL_OK, mailto: 'u+list@example.com' });
  });

  it.each([
    ['a string', 'https://example.com/u', 'must be an object'],
    ['an array', [URL_OK], 'must be an object'],
    ['an empty object', {}, 'needs a url or a mailto'],
    [
      'an unknown field',
      { url: URL_OK, headers: 'X: y' },
      "unknown field 'headers'"
    ],
    ['http', { url: 'http://example.com/u' }, 'must be https'],
    ['javascript:', { url: 'javascript:alert(1)' }, 'must be https'],
    ['not a URL', { url: 'https://' }, 'not a URL'],
    [
      'a CRLF',
      { url: 'https://example.com/u\r\nBcc: x@y.com' },
      'printable ASCII'
    ],
    ['a bare LF', { url: 'https://example.com/u\nX: 1' }, 'printable ASCII'],
    ['a space', { url: 'https://example.com/a b' }, 'printable ASCII'],
    ['non-ASCII', { url: 'https://exämple.com/u' }, 'printable ASCII'],
    [
      'a closing bracket',
      { url: 'https://example.com/u>,<https://evil.test' },
      'percent-encode'
    ],
    ['a comma', { url: 'https://example.com/u,v' }, 'percent-encode'],
    [
      'credentials',
      { url: 'https://user:pw@example.com/u' },
      'user name or password'
    ],
    [
      'an over-long url',
      {
        url: `https://example.com/${'a'.repeat(EMAIL_LIMITS.maxUnsubscribeUrlLength)}`
      },
      'longer than'
    ],
    ['a non-string url', { url: 42 }, 'url must be a string'],
    ['a malformed mailto', { mailto: 'not-an-address' }, 'not a plain mailbox'],
    ['a mailto URI', { mailto: 'mailto:u@example.com' }, 'not a plain mailbox'],
    [
      'mailto hfields',
      { mailto: 'u?subject=x@example.com' },
      'not a plain mailbox'
    ],
    [
      'a CRLF in mailto',
      { mailto: 'u@example.com\r\nBcc: x@y.com' },
      'not a plain mailbox'
    ],
    [
      'a non-boolean oneClick',
      { url: URL_OK, oneClick: 'yes' },
      'true or false'
    ],
    [
      'oneClick without url',
      { mailto: 'u@example.com', oneClick: true },
      'needs a url'
    ]
  ])('refuses %s', (_label, value, reason) => {
    expect(refusal(value).message).toContain(reason);
  });

  it('makes oneClick without a url a type error too', () => {
    // @ts-expect-error RFC 8058 posts to the url; mailto-only cannot be one-click.
    const mailtoOnly: EmailListUnsubscribe = {
      mailto: 'u@example.com',
      oneClick: true
    };
    expect(refusal(mailtoOnly).message).toContain('needs a url');
  });
});

describe('validateEmailSendRequest with listUnsubscribe', () => {
  it('puts it on the wire body only when given', () => {
    const base = { to: 'pat@example.com', subject: 'Recap', text: 'Hi' };
    expect(validateEmailSendRequest(base)).not.toHaveProperty(
      'listUnsubscribe'
    );
    expect(
      validateEmailSendRequest({
        ...base,
        listUnsubscribe: { url: URL_OK, oneClick: true }
      })
    ).toEqual({
      to: ['pat@example.com'],
      subject: 'Recap',
      text: 'Hi',
      listUnsubscribe: { url: URL_OK, oneClick: true }
    });
  });

  it('refuses a bad one like any other bad field', () => {
    expect(() =>
      validateEmailSendRequest({
        to: 'pat@example.com',
        subject: 'Recap',
        text: 'Hi',
        listUnsubscribe: { url: 'http://example.com/u' }
      })
    ).toThrow(/listUnsubscribe: url must be https/);
  });
});

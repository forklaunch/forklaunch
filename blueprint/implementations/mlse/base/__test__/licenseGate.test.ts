import {
  licenseScopeFor,
  normalizeLicense
} from '../services/licenseGate.service';

describe('normalizeLicense', () => {
  it.each([
    ['CC BY 4.0', 'cc-by'],
    ['cc-by', 'cc-by'],
    ['CC BY-SA 3.0', 'cc-by-sa'],
    ['CC0', 'cc0'],
    ['CC0 1.0', 'cc0'],
    ['http://creativecommons.org/licenses/by-sa/3.0/', 'cc-by-sa'],
    ['https://creativecommons.org/licenses/by/4.0/', 'cc-by'],
    ['https://creativecommons.org/publicdomain/zero/1.0/', 'cc0'],
    ['CC BY-NC-SA 3.0 IGO', 'cc-by-nc-sa'],
    ['Public Domain', 'public-domain'],
    ['', 'unknown'],
    [undefined, 'unknown']
  ])('normalizes %s to %s', (input, expected) => {
    expect(normalizeLicense(input)).toBe(expected);
  });
});

describe('licenseScopeFor', () => {
  it.each(['CC0', 'CC BY 4.0', 'CC BY-SA 4.0', 'Public Domain', 'US Government Work'])(
    'allows full text for commercially reusable license %s',
    (license) => {
      expect(licenseScopeFor(license)).toBe('full_text');
    }
  );

  // StatPearls is CC BY-NC-ND and WHO guidelines are CC BY-NC-SA 3.0 IGO:
  // neither may be stored in a commercial product.
  it.each([
    'CC BY-NC 4.0',
    'CC BY-NC-ND 4.0',
    'CC BY-NC-SA 3.0 IGO',
    'CC BY-ND 4.0',
    'All rights reserved'
  ])('restricts %s to metadata only', (license) => {
    expect(licenseScopeFor(license)).toBe('metadata_only');
  });

  it('treats a missing license as metadata only', () => {
    expect(licenseScopeFor(undefined)).toBe('metadata_only');
    expect(licenseScopeFor(null)).toBe('metadata_only');
  });

  it('allows excerpts for publisher-copyrighted abstracts', () => {
    expect(licenseScopeFor('publisher-copyright-abstract')).toBe(
      'excerpt_only'
    );
  });
});

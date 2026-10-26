import {
  DocumentSectionDto,
  LicenseScope
} from '@forklaunch/interfaces-mlse/types';

/**
 * What a document's license allows MLSE to keep from its text. Used both
 * when ingesting into the corpus and when showing live results, so stored
 * and live content obey exactly the same rule.
 *
 * - full_text: every section
 * - excerpt_only: the opening of the text, up to excerptChars, cut at a word
 *   boundary and marked with an ellipsis
 * - metadata_only: nothing
 */
export function applyLicense(
  sections: DocumentSectionDto[],
  licenseScope: LicenseScope,
  excerptChars = 500
): DocumentSectionDto[] {
  if (licenseScope === 'metadata_only') {
    return [];
  }
  if (licenseScope === 'full_text') {
    return sections;
  }

  const excerpt: DocumentSectionDto[] = [];
  let remaining = excerptChars;
  for (const section of sections) {
    if (remaining <= 0) {
      break;
    }
    if (section.text.length <= remaining) {
      excerpt.push(section);
      remaining -= section.text.length;
      continue;
    }
    const cut = section.text.slice(0, remaining);
    const boundary = cut.lastIndexOf(' ');
    excerpt.push({
      path: section.path,
      text: `${(boundary > 0 ? cut.slice(0, boundary) : cut).trimEnd()}…`
    });
    remaining = 0;
  }
  return excerpt;
}

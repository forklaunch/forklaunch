import { XMLParser } from 'fast-xml-parser';
import { collapseWhitespace } from './http';

// Elements whose content mixes text with inline markup (<i>, <sup>, <xref>).
// They are kept as raw XML and flattened to text, which keeps words in order;
// letting the parser split them would scatter text across child nodes.
const MIXED_CONTENT = [
  '*.AbstractText',
  '*.ArticleTitle',
  '*.article-title',
  '*.title',
  '*.p',
  '*.license-p'
];

// Elements that may appear once or many times; always parsed as arrays so
// callers never branch on the count.
const ALWAYS_ARRAY = new Set([
  'PubmedArticle',
  'AbstractText',
  'PublicationType',
  'MeshHeading',
  'ArticleId',
  'CommentsCorrections',
  'article',
  'article-id',
  'pub-date',
  'sec',
  'p',
  'DescriptorRecord',
  'TreeNumber',
  'Concept',
  'Term'
]);

export function createXmlParser(): XMLParser {
  return new XMLParser({
    ignoreAttributes: false,
    attributeNamePrefix: '@_',
    textNodeName: '#text',
    stopNodes: MIXED_CONTENT,
    isArray: (name) => ALWAYS_ARRAY.has(name),
    // keep every value as text: ids and dates like "03" must not become
    // numbers and lose their leading zeros
    parseTagValue: false,
    processEntities: true,
    // DTD declarations in NCBI responses are not needed and must not be
    // expanded.
    htmlEntities: false
  });
}

const NAMED_ENTITIES: Record<string, string> = {
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'",
  nbsp: ' '
};

function decodeEntities(text: string): string {
  return text.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (match, entity: string) => {
    if (entity[0] === '#') {
      const code =
        entity[1] === 'x' || entity[1] === 'X'
          ? parseInt(entity.slice(2), 16)
          : parseInt(entity.slice(1), 10);
      return Number.isFinite(code) ? String.fromCodePoint(code) : match;
    }
    return NAMED_ENTITIES[entity.toLowerCase()] ?? match;
  });
}

// Flattens a raw mixed-content element (as kept by stopNodes) to plain text.
export function textOf(node: unknown): string {
  if (node === undefined || node === null) {
    return '';
  }
  if (typeof node === 'string' || typeof node === 'number') {
    return collapseWhitespace(
      decodeEntities(String(node).replace(/<[^>]*>/g, ' '))
    )
      // removing inline tags leaves spaces inside brackets: "[ 1 ]" -> "[1]"
      .replace(/\s+([,.;:)\]])/g, '$1')
      .replace(/([([])\s+/g, '$1');
  }
  if (Array.isArray(node)) {
    return node.map(textOf).filter(Boolean).join(' ');
  }
  if (typeof node === 'object') {
    const record = node as Record<string, unknown>;
    if ('#text' in record) {
      return textOf(record['#text']);
    }
  }
  return '';
}

export function asArray<T>(value: T | T[] | undefined): T[] {
  if (value === undefined || value === null) {
    return [];
  }
  return Array.isArray(value) ? value : [value];
}

const MONTHS: Record<string, string> = {
  jan: '01', feb: '02', mar: '03', apr: '04', may: '05', jun: '06',
  jul: '07', aug: '08', sep: '09', oct: '10', nov: '11', dec: '12'
};

// Builds YYYY-MM-DD (or YYYY-MM / YYYY) from year/month/day parts, accepting
// numeric or abbreviated month names as NCBI uses both.
export function isoDate(
  year: unknown,
  month?: unknown,
  day?: unknown
): string | undefined {
  const y = textOf(year);
  if (!/^\d{4}$/.test(y)) {
    return undefined;
  }
  const rawMonth = textOf(month).toLowerCase();
  const m = /^\d{1,2}$/.test(rawMonth)
    ? rawMonth.padStart(2, '0')
    : MONTHS[rawMonth.slice(0, 3)];
  if (!m) {
    return y;
  }
  const d = textOf(day);
  return /^\d{1,2}$/.test(d) ? `${y}-${m}-${d.padStart(2, '0')}` : `${y}-${m}`;
}

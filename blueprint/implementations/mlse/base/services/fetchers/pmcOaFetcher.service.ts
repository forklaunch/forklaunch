import { SourceFetcher } from '@forklaunch/interfaces-mlse/interfaces';
import {
  DocumentSectionDto,
  FetchedDocumentDto,
  SourceQueryDto
} from '@forklaunch/interfaces-mlse/types';
import { FetchLike } from '../../domain/http';
import { asArray, createXmlParser, isoDate, textOf } from '../../domain/xml';
import { EutilsClient, EutilsOptions } from './eutils';

type XmlNode = Record<string, unknown>;

// Sections that add no evidence and would only dilute search.
const SKIPPED_SECTION_TYPES = new Set([
  'supplementary-material',
  'data-availability',
  'COI-statement',
  'author-contributions'
]);

/**
 * Full-text articles from the PubMed Central Open Access Subset. The search
 * is restricted to open-access articles, and each article's own license
 * (usually a Creative Commons URL) is passed through unchanged: the license
 * gate, not this fetcher, decides whether its full text may be stored.
 */
export class PmcOaFetcher implements SourceFetcher {
  readonly sourceKey = 'pmc_oa';
  private readonly eutils: EutilsClient;
  private readonly parser = createXmlParser();

  constructor(fetchImpl: FetchLike, options: EutilsOptions) {
    this.eutils = new EutilsClient(this.sourceKey, fetchImpl, options);
  }

  async fetchDocuments({
    term,
    limit
  }: SourceQueryDto): Promise<FetchedDocumentDto[]> {
    const ids = await this.eutils.search(
      'pmc',
      `(${term}) AND open access[filter]`,
      limit
    );
    if (ids.length === 0) {
      return [];
    }
    return this.parseArticles(await this.eutils.fetchXml('pmc', ids));
  }

  parseArticles(xml: string): FetchedDocumentDto[] {
    const root = this.parser.parse(xml) as XmlNode;
    const set = (root['pmc-articleset'] ?? {}) as XmlNode;
    return asArray(set.article as XmlNode[])
      .map((article) => this.toDocument(article))
      .filter((doc): doc is FetchedDocumentDto => doc !== undefined);
  }

  private toDocument(article: XmlNode): FetchedDocumentDto | undefined {
    const meta = (((article.front ?? {}) as XmlNode)['article-meta'] ?? {}) as XmlNode;
    const ids = new Map(
      asArray(meta['article-id'] as XmlNode[]).map((id) => [
        String(id['@_pub-id-type'] ?? ''),
        textOf(id)
      ])
    );
    const pmcid = ids.get('pmcid') ?? (ids.get('pmc') ? `PMC${ids.get('pmc')}` : undefined);
    if (!pmcid) {
      return undefined;
    }

    const title = textOf(((meta['title-group'] ?? {}) as XmlNode)['article-title']);
    const permissions = (meta.permissions ?? {}) as XmlNode;
    const license = this.licenseOf(permissions.license as XmlNode | undefined);

    const dates = asArray(meta['pub-date'] as XmlNode[]);
    const preferred =
      dates.find((d) => ['epub', 'pub'].includes(String(d['@_pub-type'] ?? d['@_date-type'] ?? ''))) ??
      dates[0];

    const sections: DocumentSectionDto[] = [];
    // Abstracts are either plain paragraphs or structured into titled
    // parts (Background, Methods, ...); keep both kinds.
    const abstract = asArray(meta.abstract as XmlNode | XmlNode[])[0];
    if (abstract) {
      const abstractText = asArray(abstract.p as unknown[]).map(textOf).filter(Boolean).join(' ');
      if (abstractText) {
        sections.push({ path: 'Abstract', text: abstractText });
      }
      this.collectSections(asArray(abstract.sec as XmlNode[]), ['Abstract'], sections);
    }
    this.collectSections(
      asArray(((article.body ?? {}) as XmlNode).sec as XmlNode[]),
      [],
      sections
    );

    return {
      sourceKey: this.sourceKey,
      externalId: pmcid,
      title: title || pmcid,
      url: `https://pmc.ncbi.nlm.nih.gov/articles/${pmcid}/`,
      publishedAt: preferred ? isoDate(preferred.year, preferred.month, preferred.day) : undefined,
      license,
      isCaseReport: String(article['@_article-type'] ?? '') === 'case-report',
      sections
    };
  }

  // JATS carries the license either as an ali:license_ref URL or as an
  // xlink:href on <license>; failing both, the license-type attribute.
  private licenseOf(license: XmlNode | undefined): string | undefined {
    if (!license) {
      return undefined;
    }
    const ref = license['ali:license_ref'];
    const refText = textOf(Array.isArray(ref) ? ref[0] : ref);
    if (refText) {
      return refText;
    }
    const href = license['@_xlink:href'];
    if (typeof href === 'string' && href) {
      return href;
    }
    const type = license['@_license-type'];
    return typeof type === 'string' ? type : undefined;
  }

  private collectSections(
    secs: XmlNode[],
    trail: string[],
    out: DocumentSectionDto[]
  ): void {
    for (const sec of secs) {
      if (SKIPPED_SECTION_TYPES.has(String(sec['@_sec-type'] ?? ''))) {
        continue;
      }
      const heading = textOf(sec.title);
      const path = heading ? [...trail, heading] : trail;
      const text = asArray(sec.p as unknown[]).map(textOf).filter(Boolean).join(' ');
      if (text) {
        out.push({ path: path.join(' › ') || 'Body', text });
      }
      this.collectSections(asArray(sec.sec as XmlNode[]), path, out);
    }
  }
}

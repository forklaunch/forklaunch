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

/**
 * Citations and abstracts from PubMed. Citation metadata is free to reuse,
 * but abstracts can be publisher-copyrighted, so documents carry the
 * 'publisher-copyright-abstract' license: MLSE indexes them and shows short
 * excerpts with a link to the original rather than storing them in full.
 */
export class PubMedFetcher implements SourceFetcher {
  readonly sourceKey = 'pubmed';
  private readonly eutils: EutilsClient;
  private readonly parser = createXmlParser();

  constructor(fetchImpl: FetchLike, options: EutilsOptions) {
    this.eutils = new EutilsClient(this.sourceKey, fetchImpl, options);
  }

  async fetchDocuments({
    term,
    limit
  }: SourceQueryDto): Promise<FetchedDocumentDto[]> {
    const ids = await this.eutils.search('pubmed', term, limit);
    if (ids.length === 0) {
      return [];
    }
    return this.parseArticles(await this.eutils.fetchXml('pubmed', ids));
  }

  parseArticles(xml: string): FetchedDocumentDto[] {
    const root = this.parser.parse(xml) as XmlNode;
    const set = (root.PubmedArticleSet ?? {}) as XmlNode;
    return asArray(set.PubmedArticle as XmlNode[])
      .map((article) => this.toDocument(article))
      .filter((doc): doc is FetchedDocumentDto => doc !== undefined);
  }

  private toDocument(article: XmlNode): FetchedDocumentDto | undefined {
    const citation = (article.MedlineCitation ?? {}) as XmlNode;
    const pmid = textOf(citation.PMID);
    const info = (citation.Article ?? {}) as XmlNode;
    if (!pmid) {
      return undefined;
    }

    const sections: DocumentSectionDto[] = asArray(
      ((info.Abstract ?? {}) as XmlNode).AbstractText as unknown[]
    )
      .map((part) => {
        const label =
          part && typeof part === 'object'
            ? String((part as XmlNode)['@_Label'] ?? '')
            : '';
        const text = textOf(part);
        return {
          path: label ? `Abstract — ${label.charAt(0)}${label.slice(1).toLowerCase()}` : 'Abstract',
          text
        };
      })
      .filter((section) => section.text.length > 0);

    const publicationTypes = asArray(
      ((info.PublicationTypeList ?? {}) as XmlNode).PublicationType as unknown[]
    ).map((type) => textOf(type).toLowerCase());

    const corrections = asArray(
      ((citation.CommentsCorrectionsList ?? {}) as XmlNode)
        .CommentsCorrections as XmlNode[]
    ).map((entry) => String(entry['@_RefType'] ?? ''));

    const pubDate = ((((info.Journal ?? {}) as XmlNode).JournalIssue ?? {}) as XmlNode)
      .PubDate as XmlNode | undefined;
    const articleDate = asArray(info.ArticleDate as XmlNode | XmlNode[])[0];
    const publishedAt =
      (articleDate && isoDate(articleDate.Year, articleDate.Month, articleDate.Day)) ??
      (pubDate && isoDate(pubDate.Year, pubDate.Month, pubDate.Day)) ??
      (pubDate && isoDate(textOf(pubDate.MedlineDate).slice(0, 4)));

    const meshDescriptorUis = asArray(
      ((citation.MeshHeadingList ?? {}) as XmlNode).MeshHeading as XmlNode[]
    )
      .map((heading) => heading.DescriptorName as XmlNode | undefined)
      .map((descriptor) => (descriptor ? String(descriptor['@_UI'] ?? '') : ''))
      .filter((ui) => ui.length > 0);

    return {
      sourceKey: this.sourceKey,
      externalId: pmid,
      title: textOf(info.ArticleTitle) || `PMID ${pmid}`,
      url: `https://pubmed.ncbi.nlm.nih.gov/${pmid}/`,
      publishedAt,
      license: 'publisher-copyright-abstract',
      isCaseReport: publicationTypes.includes('case reports'),
      retracted:
        publicationTypes.includes('retracted publication') ||
        corrections.includes('RetractionIn'),
      meshDescriptorUis,
      sections
    };
  }
}

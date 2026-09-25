import { MeshConceptDto } from '@forklaunch/interfaces-mlse/types';
import { asArray, createXmlParser, textOf } from '../domain/xml';

type XmlNode = Record<string, unknown>;

const RECORD_OPEN = '<DescriptorRecord';
const RECORD_CLOSE = '</DescriptorRecord>';

/**
 * Parses NLM's MeSH descriptor file (descYYYY.xml, public domain) record by
 * record. The file is several hundred megabytes, so it is never parsed whole:
 * callers feed it in chunks and receive each complete descriptor as soon as
 * its closing tag arrives.
 */
export class MeshDescriptorParser {
  private buffer = '';
  private readonly parser = createXmlParser();

  // Adds a chunk of the file and returns every descriptor it completed.
  push(chunk: string): MeshConceptDto[] {
    this.buffer += chunk;
    const concepts: MeshConceptDto[] = [];

    let end = this.buffer.indexOf(RECORD_CLOSE);
    while (end !== -1) {
      const start = this.buffer.lastIndexOf(RECORD_OPEN, end);
      const recordEnd = end + RECORD_CLOSE.length;
      if (start !== -1) {
        const concept = this.parseRecord(this.buffer.slice(start, recordEnd));
        if (concept) {
          concepts.push(concept);
        }
      }
      this.buffer = this.buffer.slice(recordEnd);
      end = this.buffer.indexOf(RECORD_CLOSE);
    }
    return concepts;
  }

  static parseAll(xml: string): MeshConceptDto[] {
    return new MeshDescriptorParser().push(xml);
  }

  private parseRecord(xml: string): MeshConceptDto | undefined {
    const parsed = this.parser.parse(xml) as XmlNode;
    const record = asArray(parsed.DescriptorRecord as XmlNode[])[0];
    if (!record) {
      return undefined;
    }

    const descriptorUi = textOf(record.DescriptorUI);
    const preferredTerm = textOf(((record.DescriptorName ?? {}) as XmlNode).String);
    if (!descriptorUi || !preferredTerm) {
      return undefined;
    }

    const synonyms = new Set<string>();
    for (const concept of asArray(
      ((record.ConceptList ?? {}) as XmlNode).Concept as XmlNode[]
    )) {
      for (const term of asArray(((concept.TermList ?? {}) as XmlNode).Term as XmlNode[])) {
        const value = textOf(term.String);
        if (value && value !== preferredTerm) {
          synonyms.add(value);
        }
      }
    }

    return {
      descriptorUi,
      preferredTerm,
      synonyms: [...synonyms],
      treeNumbers: asArray(
        ((record.TreeNumberList ?? {}) as XmlNode).TreeNumber as unknown[]
      )
        .map(textOf)
        .filter(Boolean)
    };
  }
}

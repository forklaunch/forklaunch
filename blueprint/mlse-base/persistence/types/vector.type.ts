import { Type } from '@mikro-orm/core';

/**
 * Maps a number[] embedding to a pgvector `vector` column. pgvector's text
 * form ('[0.1,0.2,...]') is also valid JSON, which makes both directions a
 * plain serialization.
 *
 * The column is declared without a dimension so the embedding model can be
 * chosen later without a schema change; a dimension-specific index is added
 * once it is.
 */
export class VectorType extends Type<number[] | null, string | null> {
  override convertToDatabaseValue(value: number[] | null): string | null {
    if (value === null || value === undefined) {
      return null;
    }
    if (!Array.isArray(value) || !value.every((v) => Number.isFinite(v))) {
      throw new Error('An embedding must be an array of finite numbers');
    }
    return `[${value.join(',')}]`;
  }

  override convertToJSValue(value: string | number[] | null): number[] | null {
    if (value === null || value === undefined) {
      return null;
    }
    return Array.isArray(value) ? value : (JSON.parse(value) as number[]);
  }

  override getColumnType(): string {
    return 'vector';
  }
}

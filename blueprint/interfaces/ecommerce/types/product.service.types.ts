import { IdDto, IdsDto, RecordTimingDto } from '@forklaunch/common';

/**
 * A product option dimension (e.g. "Color", "Size"). `isPackQuantity` flags the
 * case surfaced in migration testing where a "Size"-named option actually
 * encodes pack quantity (1 Can / 4 Pack / 12 Pack) rather than a physical size,
 * and where pack semantics appear under arbitrary names ("Size", "Cans").
 */
export type ProductOption = {
  name: string;
  isPackQuantity: boolean;
  values: string[];
};

export type ProductImage = {
  src: string;
  position: number;
};

export type CreateProductDto = Partial<IdDto> & {
  /** Stable id from the source platform; kept so re-imports dedupe. */
  externalId: string;
  /** URL slug — preserved so SEO redirects can map old links later. */
  handle: string;
  /** Full original product URL, for redirect mapping. */
  sourceUrl?: string;
  title: string;
  descriptionHtml?: string;
  vendor?: string;
  productType?: string;
  tags?: string[];
  /** 0, 1, or 2+ option dimensions with arbitrary names. */
  options?: ProductOption[];
  /** CDN image URLs (media ingestion is the migration tool's concern). */
  images?: ProductImage[];
};

export type UpdateProductDto = Partial<CreateProductDto> & IdDto;

export type ProductDto = CreateProductDto & IdDto & Partial<RecordTimingDto>;

/**
 * Catalog search/filter (ECOM-03). `ids` narrows to specific products,
 * everything else is optional and additive. Price/stock/option filters are
 * variant-level under the hood (Product itself carries no price) — a
 * product matches if at least one of its variants satisfies them.
 */
export type ProductSearchDto = Partial<IdsDto> & {
  /** Case-insensitive partial match against the product title. */
  title?: string;
  minPriceCents?: number;
  maxPriceCents?: number;
  /** True: at least one variant has stock > 0. */
  inStock?: boolean;
  /** Both must be set together — e.g. optionName 'Color', optionValue 'Black'. */
  optionName?: string;
  optionValue?: string;
};

export type ProductServiceParameters = {
  CreateProductDto: CreateProductDto;
  UpdateProductDto: UpdateProductDto;
  ProductDto: ProductDto;
  IdDto: IdDto;
  IdsDto: IdsDto;
  SearchDto: ProductSearchDto;
};

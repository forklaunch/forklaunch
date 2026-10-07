import { IdDto, IdsDto, RecordTimingDto } from '@forklaunch/common';

export type CreateVariantDto = Partial<IdDto> & {
  /** Owning product id (module-side). */
  productId: string;
  /** Stable id from the source platform; the reliable dedupe key. */
  externalId: string;
  /**
   * SKU is frequently absent in real source feeds (many merchants never set
   * one), so it is optional — the platform externalId is the reliable key.
   */
  sku?: string;
  title: string;
  /** Cleaned option-name -> value, e.g. { "Color": "Black" }. */
  optionValues?: Record<string, string>;
  /** Price in integer minor units (cents) — never a float. */
  priceCents: number;
  /** Original ("compare at") price in cents when on sale. */
  compareAtPriceCents?: number;
  requiresShipping?: boolean;
};

export type UpdateVariantDto = Partial<CreateVariantDto> & IdDto;

export type VariantDto = CreateVariantDto & IdDto & Partial<RecordTimingDto>;

export type VariantServiceParameters = {
  CreateVariantDto: CreateVariantDto;
  UpdateVariantDto: UpdateVariantDto;
  VariantDto: VariantDto;
  IdDto: IdDto;
  IdsDto: IdsDto;
};

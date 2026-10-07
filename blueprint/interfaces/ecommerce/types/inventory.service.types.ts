import { IdDto, RecordTimingDto } from '@forklaunch/common';

export type CreateInventoryDto = Partial<IdDto> & {
  variantId: string;
  stock: number;
};

export type UpdateInventoryDto = Partial<CreateInventoryDto> & IdDto;

export type InventoryDto = CreateInventoryDto & IdDto & Partial<RecordTimingDto>;

/** Manual stock adjustment; `delta` may be negative. No reservation in v1. */
export type AdjustStockDto = {
  variantId: string;
  delta: number;
};

/** Synchronous "in stock?" check used at checkout. */
export type StockCheckDto = {
  variantId: string;
  requested: number;
};

export type StockCheckResultDto = {
  variantId: string;
  available: boolean;
  stock: number;
};

export type InventoryServiceParameters = {
  CreateInventoryDto: CreateInventoryDto;
  UpdateInventoryDto: UpdateInventoryDto;
  InventoryDto: InventoryDto;
  AdjustStockDto: AdjustStockDto;
  StockCheckDto: StockCheckDto;
  StockCheckResultDto: StockCheckResultDto;
  IdDto: IdDto;
};

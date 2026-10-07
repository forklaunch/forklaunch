import {
  CartDto,
  CreateCartDto,
  CreateInventoryDto,
  CreateOrderDto,
  CreatePaymentDto,
  CreateProductDto,
  CreateVariantDto,
  InventoryDto,
  OrderDto,
  PaymentDto,
  ProductDto,
  UpdateCartDto,
  UpdateInventoryDto,
  UpdateOrderDto,
  UpdateProductDto,
  UpdateVariantDto,
  VariantDto
} from '@forklaunch/interfaces-ecommerce/types';

// product dto types
export type BaseProductDtos = {
  ProductMapper: ProductDto;
  CreateProductMapper: CreateProductDto;
  UpdateProductMapper: UpdateProductDto;
};

// variant dto types
export type BaseVariantDtos = {
  VariantMapper: VariantDto;
  CreateVariantMapper: CreateVariantDto;
  UpdateVariantMapper: UpdateVariantDto;
};

// inventory dto types
export type BaseInventoryDtos = {
  InventoryMapper: InventoryDto;
  CreateInventoryMapper: CreateInventoryDto;
  UpdateInventoryMapper: UpdateInventoryDto;
};

// cart dto types
export type BaseCartDtos = {
  CartMapper: CartDto;
  CreateCartMapper: CreateCartDto;
  UpdateCartMapper: UpdateCartDto;
};


// order dto types
export type BaseOrderDtos = {
  OrderMapper: OrderDto;
  CreateOrderMapper: CreateOrderDto;
  UpdateOrderMapper: UpdateOrderDto;
};

// payment dto types
export type BasePaymentDtos = {
  PaymentMapper: PaymentDto;
  CreatePaymentMapper: CreatePaymentDto;
};

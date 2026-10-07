import { IdDto, RecordTimingDto } from '@forklaunch/common';

export type CartItemDto = {
  variantId: string;
  quantity: number;
};

export type CreateCartDto = Partial<IdDto> & {
  customerId?: string;
};

export type UpdateCartDto = Partial<CreateCartDto> & IdDto;

export type CartDto = CreateCartDto &
  IdDto &
  Partial<RecordTimingDto> & {
    status: string;
    items: CartItemDto[];
  };

export type AddCartItemDto = {
  cartId: string;
  variantId: string;
  quantity: number;
};

export type RemoveCartItemDto = {
  cartId: string;
  variantId: string;
};

export type CartServiceParameters = {
  CreateCartDto: CreateCartDto;
  UpdateCartDto: UpdateCartDto;
  CartDto: CartDto;
  AddCartItemDto: AddCartItemDto;
  RemoveCartItemDto: RemoveCartItemDto;
  IdDto: IdDto;
};

import { IdDto, IdsDto, RecordTimingDto } from '@forklaunch/common';

export type CreateSubscriptionDto<PartyType, BillingProviderType> =
  Partial<IdDto> & {
    partyId: string;
    partyType: PartyType[keyof PartyType];
    description?: string;
    active: boolean;
    productId: string;
    providerFields?: unknown;
    externalId: string;
    billingProvider: BillingProviderType[keyof BillingProviderType];
    startDate: Date;
    endDate?: Date;
    status: string;
  };
export type UpdateSubscriptionDto<PartyType, BillingProviderType> = Partial<
  CreateSubscriptionDto<PartyType, BillingProviderType>
> &
  IdDto;
export type SubscriptionDto<PartyType, BillingProviderType> =
  CreateSubscriptionDto<PartyType, BillingProviderType> &
    IdDto &
    Partial<RecordTimingDto>;

export type SubscriptionServiceParameters<PartyType, BillingProviderType> = {
  CreateSubscriptionDto: CreateSubscriptionDto<PartyType, BillingProviderType>;
  UpdateSubscriptionDto: UpdateSubscriptionDto<PartyType, BillingProviderType>;
  SubscriptionDto: SubscriptionDto<PartyType, BillingProviderType>;
  IdDto: IdDto;
  IdsDto: IdsDto;
};

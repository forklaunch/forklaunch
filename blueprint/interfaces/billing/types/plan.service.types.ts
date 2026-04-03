import { IdDto, IdsDto, RecordTimingDto } from '@forklaunch/common';

export type CreatePlanDto<PlanCadenceEnum, CurrencyEnum, BillingProviderEnum> =
  Partial<IdDto> & {
    active: boolean;
    name: string;
    description?: string;
    price: number;
    currency: CurrencyEnum[keyof CurrencyEnum];
    cadence: PlanCadenceEnum[keyof PlanCadenceEnum];
    features?: string[];
    providerFields?: unknown;
    externalId: string;
    billingProvider: BillingProviderEnum[keyof BillingProviderEnum];
  };
export type UpdatePlanDto<PlanCadenceEnum, CurrencyEnum, BillingProviderEnum> =
  Partial<CreatePlanDto<PlanCadenceEnum, CurrencyEnum, BillingProviderEnum>> &
    IdDto;
export type PlanDto<PlanCadenceEnum, CurrencyEnum, BillingProviderEnum> =
  CreatePlanDto<PlanCadenceEnum, CurrencyEnum, BillingProviderEnum> &
    IdDto &
    Partial<RecordTimingDto>;

export type PlanServiceParameters<
  PlanCadenceEnum,
  CurrencyEnum,
  BillingProviderEnum
> = {
  CreatePlanDto: CreatePlanDto<
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum
  >;
  UpdatePlanDto: UpdatePlanDto<
    PlanCadenceEnum,
    CurrencyEnum,
    BillingProviderEnum
  >;
  PlanDto: PlanDto<PlanCadenceEnum, CurrencyEnum, BillingProviderEnum>;
  IdDto: IdDto;
  IdsDto: IdsDto;
};

import { LiteralSchema } from '@forklaunch/validator';
import {
  array,
  boolean,
  date,
  enum_,
  number,
  optional,
  string,
  unknown,
  uuid
} from '@forklaunch/validator/zod';

export const CreatePlanSchema = <
  T extends Record<string, LiteralSchema>,
  U extends Record<string, LiteralSchema>,
  V extends Record<string, LiteralSchema>
>(
  PlanCadenceEnum: T,
  CurrencyEnum: U,
  BillingProviderEnum: V
) => ({
  name: string,
  description: optional(string),
  price: number,
  cadence: enum_(PlanCadenceEnum),
  currency: enum_(CurrencyEnum),
  features: optional(array(string)),
  externalId: string,
  billingProvider: enum_(BillingProviderEnum),
  active: boolean,
  providerFields: optional(unknown)
});

export const UpdatePlanSchema =
  ({ uuidId }: { uuidId: boolean }) =>
  <
    T extends Record<string, LiteralSchema>,
    U extends Record<string, LiteralSchema>,
    V extends Record<string, LiteralSchema>
  >(
    PlanCadenceEnum: T,
    CurrencyEnum: U,
    BillingProviderEnum: V
  ) => ({
    id: uuidId ? uuid : string,
    name: optional(string),
    description: optional(string),
    price: optional(number),
    cadence: optional(enum_(PlanCadenceEnum)),
    currency: optional(enum_(CurrencyEnum)),
    features: optional(array(string)),
    externalId: optional(string),
    billingProvider: optional(enum_(BillingProviderEnum)),
    active: optional(boolean),
    providerFields: optional(unknown)
  });

export const PlanSchema =
  ({ uuidId }: { uuidId: boolean }) =>
  <
    T extends Record<string, LiteralSchema>,
    U extends Record<string, LiteralSchema>,
    V extends Record<string, LiteralSchema>
  >(
    PlanCadenceEnum: T,
    CurrencyEnum: U,
    BillingProviderEnum: V
  ) => ({
    id: uuidId ? uuid : string,
    name: string,
    description: optional(string),
    price: number,
    cadence: enum_(PlanCadenceEnum),
    currency: enum_(CurrencyEnum),
    features: optional(array(string)),
    externalId: string,
    billingProvider: enum_(BillingProviderEnum),
    active: boolean,
    providerFields: optional(unknown),
    createdAt: optional(date),
    updatedAt: optional(date)
  });

export const BasePlanServiceSchemas = (options: { uuidId: boolean }) => ({
  CreatePlanSchema,
  UpdatePlanSchema: UpdatePlanSchema(options),
  PlanSchema: PlanSchema(options)
});

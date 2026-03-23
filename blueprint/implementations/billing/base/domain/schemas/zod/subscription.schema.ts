import { LiteralSchema } from '@forklaunch/validator';
import {
  boolean,
  date,
  enum_,
  optional,
  string,
  unknown,
  uuid
} from '@forklaunch/validator/zod';

export const CreateSubscriptionSchema = <
  T extends Record<string, LiteralSchema>,
  U extends Record<string, LiteralSchema>
>(
  PartyEnum: T,
  BillingProviderEnum: U
) => ({
  partyId: string,
  partyType: enum_(PartyEnum),
  productId: string,
  description: optional(string),
  active: boolean,
  externalId: string,
  startDate: date,
  endDate: optional(date),
  status: string,
  billingProvider: enum_(BillingProviderEnum),
  providerFields: optional(unknown)
});

export const UpdateSubscriptionSchema =
  ({ uuidId }: { uuidId: boolean }) =>
  <
    T extends Record<string, LiteralSchema>,
    U extends Record<string, LiteralSchema>
  >(
    PartyEnum: T,
    BillingProviderEnum: U
  ) => ({
    id: uuidId ? uuid : string,
    partyId: optional(string),
    partyType: optional(enum_(PartyEnum)),
    productId: optional(string),
    description: optional(string),
    active: optional(boolean),
    externalId: optional(string),
    startDate: optional(date),
    endDate: optional(date),
    status: optional(string),
    billingProvider: optional(enum_(BillingProviderEnum)),
    providerFields: optional(unknown)
  });

export const SubscriptionSchema =
  ({ uuidId }: { uuidId: boolean }) =>
  <
    T extends Record<string, LiteralSchema>,
    U extends Record<string, LiteralSchema>
  >(
    PartyEnum: T,
    BillingProviderEnum: U
  ) => ({
    id: uuidId ? uuid : string,
    partyId: string,
    partyType: enum_(PartyEnum),
    productId: string,
    description: optional(string),
    active: boolean,
    externalId: string,
    startDate: date,
    endDate: optional(date),
    status: string,
    billingProvider: enum_(BillingProviderEnum),
    providerFields: optional(unknown),
    createdAt: optional(date),
    updatedAt: optional(date)
  });

export const BaseSubscriptionServiceSchemas = (options: {
  uuidId: boolean;
}) => ({
  CreateSubscriptionSchema,
  UpdateSubscriptionSchema: UpdateSubscriptionSchema(options),
  SubscriptionSchema: SubscriptionSchema(options)
});

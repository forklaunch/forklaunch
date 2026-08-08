/**
 * Sourced directly from @forklaunch/validator/zod and @forklaunch/express —
 * the actual origin of these constructs — instead of the app-level
 * @forklaunch/blueprint-core aggregator. blueprint/core/registrations.ts has
 * a pre-existing bug (a type-only import written as a value import) that
 * crashes anything loaded via tsx-on-source, including the shipped billing
 * module. This file lets ecommerce-stripe run without depending on that fix.
 */
import {
  array,
  optional,
  SchemaValidator as SchemaValidatorFactory,
  string
} from '@forklaunch/validator/zod';

export {
  array,
  boolean,
  date,
  enum_,
  number,
  optional,
  record,
  string,
  unknown,
  uuid
} from '@forklaunch/validator/zod';
export {
  forklaunchExpress,
  forklaunchRouter,
  handlers
} from '@forklaunch/express';
export { PERMISSIONS, ROLES } from '@forklaunch/blueprint-core';

export const schemaValidator = SchemaValidatorFactory();
export type SchemaValidator = ReturnType<typeof SchemaValidatorFactory>;

export const IdSchema = { id: string };
// Optional — every "list all" endpoint (Review, Subscription, PromoCode,
// GiftCard, ...) uses this as its query type, and "list everything, no
// filter" is the common case. Without `optional()` here, calling any of
// those endpoints with no `ids` query param always 400s — this was a real,
// pre-existing bug affecting every list-all endpoint in the module, not
// something specific to one feature.
export const IdsSchema = { ids: optional(array(string)) };

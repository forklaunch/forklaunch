/**
 * Re-exported from the app's core package, like every other module, so the
 * ecommerce module uses the app's validator (zod or typebox) and HTTP
 * framework (Express or hyper-express). It used to import
 * @forklaunch/validator/zod and @forklaunch/express directly, which broke
 * every typebox or hyper-express app (TS2883, TS7006). The core
 * registrations bug that workaround avoided (type-only exports written as
 * values) is fixed.
 */
import { array, optional, string } from '@forklaunch/blueprint-core';

export {
  array,
  boolean,
  date,
  enum_,
  forklaunchExpress,
  forklaunchRouter,
  handlers,
  number,
  optional,
  PERMISSIONS,
  record,
  ROLES,
  schemaValidator,
  string,
  unknown,
  uuid
} from '@forklaunch/blueprint-core';
export type { SchemaValidator } from '@forklaunch/blueprint-core';

export const IdSchema = { id: string };
// Optional — every "list all" endpoint (Review, Subscription, PromoCode,
// GiftCard, ...) uses this as its query type, and "list everything, no
// filter" is the common case. Without `optional()` here, calling any of
// those endpoints with no `ids` query param always 400s — this was a real,
// pre-existing bug affecting every list-all endpoint in the module, not
// something specific to one feature.
export const IdsSchema = { ids: optional(array(string)) };

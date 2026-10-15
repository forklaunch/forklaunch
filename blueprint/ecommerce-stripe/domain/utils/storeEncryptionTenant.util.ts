/**
 * The tenant this service's encrypted columns are sealed under.
 *
 * Carts, orders and the catalog belong to the store, not to an organization:
 * no entity here has a tenant column, and the routes are internal HMAC calls
 * that carry no tenant. `@forklaunch/core` 3 refuses to encrypt with no tenant
 * bound (`UnboundTenantError`) and no longer treats `''` as "global", so
 * without a tenant of its own every order write (`Order.shippingAddress` is
 * PII) failed. Store rows are sealed under this explicit, searchable id
 * instead, the way core asks rows that belong to no organization to be.
 *
 * A caller that does resolve a tenant still passes it in the DI context and
 * wins (see the EntityManager factory in registrations.ts).
 */
export const STORE_ENCRYPTION_TENANT = 'ecommerce:store';

# Optional checkout session URI

Stripe [Checkout Session URLs](https://docs.stripe.com/api/checkout/sessions/object#checkout_session_object-url) are nullable. Checkout creation and responses now allow an absent `uri`, matching the native Rust optional field. A missing provider URL becomes an omitted DTO property (including JSON cache responses) and SQL `NULL` when database backup is enabled. Existing URL strings are preserved. This does not change checkout customer identity or establish provider account ownership.

Callers must check `session.uri` before redirecting. A missing URL does not prove payment success or failure. The configured response mapper converts database `NULL` to `undefined`; the public schema accepts absence, not explicit JSON `null`.

## Existing applications with database backup

Updating the entity source does not migrate an existing database. Before deploying code that stores sessions without URLs:

1. Update the checkout entity to nullable `uri` and the configured create/response mappers together with the Stripe DTO/schema changes.
2. Run `pnpm run migrate:create` from the generated billing service using the intended database configuration. Review the generated migration and retain only the intended schema changes.
3. For the default PostgreSQL table, the relevant change is:

   ```sql
   ALTER TABLE checkout_session ALTER COLUMN uri DROP NOT NULL;
   ```

   Use the actual schema and table name if your application customizes them. Existing URI values are retained; do not replace absent URLs with empty strings or fabricated URLs.
4. Apply the reviewed migration with `pnpm run migrate:up` before deploying the new writer. Check that a session with no provider URL can be stored and read, and that a session with a URL still roundtrips unchanged.

Cache-only applications need the DTO/schema/mapper and caller updates but no database migration for this field. This guide does not run migrations or contact Stripe.

A rollback to the old required-URI schema needs an explicit decision for every existing `NULL` row before restoring `NOT NULL`; blindly applying a down migration will fail when such rows exist. Older application readers may reject these sessions, so coordinate application rollback as well.

## Repository verification

`framework-rs/parity/rust_billing_checkout_uri.mjs` exercises actual Zod and TypeBox schemas and configured mapper/service JSON-cache lifecycles. With `FORKLAUNCH_TEST_POSTGRES_URL` and `FORKLAUNCH_TEST_ORM_DEPENDENCIES`, it also tests the actual entity through MikroORM using an isolated restricted PostgreSQL role, including the old `NOT NULL` rejection and the reviewed constraint removal. It never calls a live payment provider.

# Configured payment-link reads

The configured TypeScript mappers expose public UUIDs while retaining Stripe `plink_` references in cached provider fields. GET and list use those references for read-only Stripe retrieval and preserve the public IDs. Cache evidence alone does not prove provider account ownership; native explicitly provisioned, tenant-scoped aliases and the managed account-generation protocol remain separate mechanisms. Provider mutations do not use these cache hooks, but they do use the stored Stripe id: update, expire, success and failure read the cached record and call Stripe with its stored `plink_` id (checkout expire, success and failure likewise use the stored `cs_` id). A record without a stored Stripe id fails before any Stripe call.

List behavior with the configured `readCachedDto` and `validateCachedRead` hooks:

- Nonempty `ids` select those cached public IDs in request order; duplicates remain duplicates. Omitted `ids` and `ids: []` select all cache keys, preserving the existing base-service meaning.
- Missing cache entries, mismatched cache-key IDs, malformed DTOs or invalid retained provider evidence fail the whole call before any provider GET. No partial list is returned.
- Reads run sequentially for each selected reference; no provider account-wide list or first-page matching is used. Fresh provider evidence must match reference/object/live mode. Only an actual boolean `active: true` is included; `false` is filtered and malformed active states fail the call.
- `maxConfiguredListRecords` is a trusted service-constructor option, default **100**, allowing safe integers **1–1000**. Exceeding it fails before cache-record/provider reads. To read more, select explicit ID batches or deliberately raise the trusted limit; results are never silently truncated. For example, configure `{ maxConfiguredListRecords: 150 }` to read 150 selected cache records sequentially.
- Provider failure fails the call. Cache records and database entities are not updated by the read. Neither the cache nor provider reads form a global snapshot, so records may change between requests.

The cache's existing `listKeys` API still enumerates keys when IDs are omitted; this change bounds record reads and provider traffic, not that enumeration. Use explicit ID batches for large caches. Custom mappers lacking either configured list hook retain their existing provider-ID list behavior. Native default service IDs and list behavior are unchanged.

The actual-source configured mapper fixture covers lists beyond a Stripe response page, ordering/duplicates, bounded failures, malformed/missing cache, provider failures, active filtering and custom fallback. It uses owned provider fixtures, not live Stripe calls.

## Create request structure

Stripe payment-link create requests require a non-null `stripeFields` object with a `line_items` array of objects. Zod, TypeBox, the TypeScript wrapper and the native Stripe wrapper check this structure before provider I/O. The generic base billing DTO still permits absent provider fields. See the [Stripe create API](https://docs.stripe.com/api/payment-link/create) and the pinned SDK's required `line_items` member.

Nested price/quantity fields and arbitrary provider extensions are preserved exactly and remain subject to Stripe validation; these checks do not model the complete SDK. Empty arrays pass this structural check because it does not invent a minimum item count. Applications relying on omitted provider options must supply their actual line items. Response and update payload schemas are unchanged, and this adds no authority to mutate an existing provider resource.

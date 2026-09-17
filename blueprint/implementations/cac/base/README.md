# @forklaunch/implementation-cac-base

Concrete services for the `cac` module: procedure-code lookup, and the
claim-scrubbing engine.

## ForkLaunch never holds real CPT content — read this before you touch `CptCodeProvider`

**ForkLaunch does not apply for, hold, or distribute a real AMA CPT
license — not for itself, and not on behalf of anyone using this module.**
`cac-base` ships free-first: ICD-10-CM, HCPCS, and the three-layer scrubbing
engine are fully built and usable with zero license. Real CPT procedure
codes are different — AMA owns them, and using real CPT data commercially
requires your own license from AMA (an end-user or distributor license,
depending on your product — see
`plan/cac/MEDICAL-CODING-IMPLEMENTATION-PLAN.md` §2 if you're the one
adopting this module and don't have one yet).

If your organization already holds that license, this package gives you a
real, tested extension point to plug your own licensed CPT data into — not
a stub, not a TODO, not something you have to build the interface for
yourself.

## `CodeSetProvider` — the extension point

```
CodeSetProvider (interface, @forklaunch/interfaces-cac)
 ├─ MockProcedureCodeProvider   — free, built-in, fully functional today.
 │                                 Three placeholder codes (PROC-001/002/003).
 │                                 No license needed, ever.
 └─ CptCodeProvider             — the real-CPT adapter. Complete and
                                    production-shaped, but it never contains,
                                    fetches, or ships any real AMA content.
```

### How to actually turn real CPT on

`CptCodeProvider` doesn't hold any data itself — it delegates every lookup
to a `CptCodeSource` you supply:

```ts
export interface CptCodeSource {
  lookup(code: string): Promise<ProcedureCodeDto | undefined>;
}
```

`cac-base` ships one ready-to-use implementation,
`EntityManagerCptCodeSource` (`cac-base/services/cptCodeSource.service.ts`),
backed by the `CptCode` reference table
(`cac-base/persistence/entities/cptCode.entity.ts`) — org-scoped, since
different organizations may hold licenses for different CPT
editions/vintages. That table starts empty. To populate it with **your own**
real, licensed CPT data:

1. Get your CPT data into a file `scripts/refresh-code-sets.ts` can read
   (CSV today — see `persistence/etl/cpt.loader.ts` and
   `persistence/etl/csvRowSource.ts` if your feed needs a different parser;
   the loader shape is deliberately pluggable because there's no one
   standard file format for a real CPT feed the way there is for CMS/CDC's
   free releases).
2. Set `CPT_SOURCE_PATH`, `CPT_ORGANIZATION_ID`, and (if your file's column
   order differs from the default) `CPT_CODE_COLUMN` /
   `CPT_DESCRIPTION_COLUMN` / `CPT_HAS_HEADER`.
3. Run `scripts/refresh-code-sets.ts`. It upserts into `CptCode`, keyed on
   `(organizationId, code)` — safe to re-run on whatever cadence your license
   agreement requires you to refresh CPT data.
4. Mark your organization's license active — insert or update a
   `CodeSetLicense` row (`cac-base/persistence/entities/codeSetLicense.entity.ts`)
   for your `organizationId` with `codeSetType: CodeSetType.CPT` and
   `status: LicenseStatus.ACTIVE`:

   ```ts
   // paths below are relative to cac-base/, wherever your own script lives
   import { CodeSetType } from 'domain/enum/codeSetType.enum';
   import { LicenseStatus } from 'domain/enum/licenseStatus.enum';
   import { CodeSetLicense } from 'persistence/entities/codeSetLicense.entity';

   await em.upsert(CodeSetLicense, {
     organizationId,
     codeSetType: CodeSetType.CPT,
     status: LicenseStatus.ACTIVE,
     signedAt: new Date()
   });
   ```

   That's it — no code change, no redeploy, no flag to flip per-request.

The per-organization runtime switch **is wired up**: `CodeSetProviderResolver`
(`cac-base/services/codeSetProviderResolver.service.ts`) resolves it fresh on
every request, reading `organizationId` off the caller's session. This is
deliberately **not** built on `hasFeatureChecks`/`surfaceFeatures` (the
mechanism `billing-base` uses for entitlement gating) — that guard *blocks*
a route outright when a feature is missing, which is wrong here, since real
CPT is never a hard requirement for these endpoints to work. An organization
with no `CodeSetLicense` row, a `PENDING` one, or a lookup that errors all
transparently keep getting `MockProcedureCodeProvider` — never a 403, never
a broken request. Only a `CodeSetLicense` row with `status: 'active'` flips
that organization over to `CptCodeProvider` + `EntityManagerCptCodeSource`.

Verify which provider is active for your organization at any time via the
`codeSet` routes (`admin:manage_codesets` permission required):
`GET /codeSet` returns `{ codeSetType, licensed }` for the caller's own
organization; `GET /codeSet/:code` runs an actual lookup against whichever
provider is currently active.

**Never mistake the shipped `CptCodeProvider` class for a working CPT
dataset.** It is an adapter shape, not data. It returns nothing useful until
you point a `CptCodeSource` at your own real, licensed content.

### If you don't have a CPT license yet

Nothing above applies to you yet — keep building against
`MockProcedureCodeProvider`. It's free, fully functional, and the scrubbing
engine behaves identically either way (§5); only the code data underneath
changes once you do get a license.

### Historical claims are never retroactively recoded

Once your organization's `CodeSetLicense` flips to active, only *new*
encounters use the real provider. Claims already built and submitted under
`MockProcedureCodeProvider` are never rewritten — a submitted claim is a
financial/legal record. Pass this along to your own end customers if you
build a pilot/demo flow on top of this module.

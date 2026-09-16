---
name: encryption-key-rotation
description: "Encryption keys: the key ring (ENCRYPTION_KEY + LEGACY_ENCRYPTION_KEYS), rotating a key without downtime, the re-encryption sweep migration, reading its report, and recovering when a key was replaced under live data."
user-invokable: true
---

# Encryption Key Rotation and Migration

## When to Use This Skill

Use when the user asks about, or you see:

- Rotating or changing `ENCRYPTION_KEY` for a service
- `DecryptionError`, `Failed to decrypt encrypted column value`, or
  `Cannot decrypt FieldEncryptor format` in logs
- Sign-in returning 500 with `Failed to decrypt private key` (better-auth JWKS)
- A deploy reporting that stored configuration "could not be resolved" right
  after a stack was re-created or a deploy replaced secrets
- Data written before a date being unreadable while newer data works
- `LEGACY_ENCRYPTION_KEYS`, `LEGACY_ENCRYPTION_KEY`, `reencryptEncryptedColumns`
- "Did we lose the encryption key?"

## The Model, In Plain Terms

Every compliance field (`pii`, `phi`, `pci`) is encrypted with AES-256-GCM
under a key derived from the service's `ENCRYPTION_KEY` and the row's tenant.
A value can only ever be opened with the exact key it was written under; the
wrong key is rejected outright, it never produces garbage.

Concrete example. Amy's org signs up in August; her user row is written under
key A. On Sept 3 something replaces the service's key with key B (a stack
re-creation, a manual change). Every row written from then on is under key B.
On Sept 15 the key goes back to A. The table now holds rows under A and rows
under B. A service that only knows A can read August and everything after
Sept 15, and nothing in between. Nothing is corrupt; the service simply does
not hold the key those rows need.

Two consequences drive everything below:

1. **A key must never disappear while data still depends on it.** Keep every
   key that ever wrote data until a sweep has rewritten that data.
2. **Rotation is two steps, not one.** First make the service able to read
   both keys. Then rewrite the old rows under the new key. Only then drop the
   old key.

## The Key Ring

`FieldEncryptor` holds a ring: one current key, any number of previous keys.

| Variable | Role |
| --- | --- |
| `ENCRYPTION_KEY` | Current key. Every write uses it. |
| `LEGACY_ENCRYPTION_KEYS` | Previous keys, comma separated. Read-only: tried in order when the current key does not open a value. |
| `LEGACY_ENCRYPTION_KEY` | Older single-key spelling. Still honoured; prefer the list. |

```ts
import { FieldEncryptor } from '@forklaunch/core/persistence';

const encryptor = FieldEncryptor.fromEnv(); // ENCRYPTION_KEY + LEGACY_ENCRYPTION_KEYS
// or explicitly
new FieldEncryptor(currentKey, { previousKeys: [previousKey] });

encryptor.keyIds;                  // { current: 'a1b2…', previous: ['c3d4…'] } — fingerprints, safe to log
encryptor.open(value, tenantId);   // { plaintext, keyId, current } — which key opened it
encryptor.needsRotation(value, tenantId);
encryptor.rotate(value, tenantId); // same value back if already current
```

The on-disk format does not change with the ring, so equality lookups on
rows already under the current key keep working during a rotation.

Services scaffolded by the CLI declare `LEGACY_ENCRYPTION_KEYS` as optional
config and pass it through in `registrations.ts` and `mikro-orm.config.ts`.
A service scaffolded before that must add the variable and pass
`{ previousKeys: parseEncryptionKeyList(LEGACY_ENCRYPTION_KEYS) }` wherever
it constructs a `FieldEncryptor` (cache, object store, `registerEncryptor`).

## Rotating a Key (Planned)

1. **Widen the ring.** Append the current key to `LEGACY_ENCRYPTION_KEYS`,
   set the new value in `ENCRYPTION_KEY`, deploy. Reads keep working because
   the old key is still in the ring; new writes use the new key.

   ```bash
   forklaunch config set -r <region> -e <env> -s <service> "LEGACY_ENCRYPTION_KEYS=<old-key>"
   forklaunch config set -r <region> -e <env> -s <service> "ENCRYPTION_KEY=<new-key>"
   forklaunch deploy create --release <release> --environment <env> --region <region>
   ```

   Never paste a key into chat, a commit, or a log. Read it from wherever it
   already lives and hand it straight to `config set`.

2. **Sweep.** Ship a migration that rewrites everything still under a
   previous key. It runs inside the service, so keys arrive the same way
   they do at runtime and never leave the deployment.

   ```ts
   import { Migration } from '@mikro-orm/migrations';
   import {
     FieldEncryptor,
     collectFallbackTenantIds,
     reencryptEncryptedColumns,
     rotationTotals
   } from '@forklaunch/core/persistence';

   export class Migration20260916000000_rotate_encryption_key extends Migration {
     override async up(): Promise<void> {
       const execute = (sql: string, params?: unknown[]) => this.execute(sql, params);
       const reports = await reencryptEncryptedColumns({
         em: this.getEntityManager(),
         execute, // the migrator's transaction; a pooled connection cannot see uncommitted tables
         encryptor: FieldEncryptor.fromEnv(),
         fallbackTenantIds: await collectFallbackTenantIds(execute),
         log: (m) => console.info(m)
       });
       const totals = rotationTotals(reports);
       if (totals.unreadable > 0) {
         throw new Error(`${totals.unreadable} value(s) open with no key in the ring; see the log for row ids`);
       }
     }
     override async down(): Promise<void> {}
   }
   ```

   The sweep is idempotent. Re-running it after everything is current
   rewrites nothing.

3. **Narrow the ring.** When the report shows nothing rewritten and nothing
   unreadable, remove the old key:

   ```bash
   forklaunch config unset -r <region> -e <env> -s <service> LEGACY_ENCRYPTION_KEYS
   ```

## Stamped Values (`v3`)

`v1`/`v2` values name no key, so a read has to try every key in the ring and
a decrypt failure cannot say which key is missing. The `v3` envelope,
`v3:{keyId}:{iv}:{tag}:{data}`, stamps each value with the fingerprint of the
key that wrote it. Reads resolve the key directly, a missing key fails by
name ("stamped with key c3d4…, which is not in the ring"), and the rows
still under a key can be counted without decrypting anything
(`countValuesByKeyId`, or `WHERE value LIKE 'v3:c3d4…:%'`).

Writing `v3` is opt-in: `ENCRYPTION_FORMAT=v3` (or `format: 'v3'`).
Reads accept every envelope regardless of the setting.

**Sequencing rule.** Deterministic encryption means an equality lookup
compares ciphertext bytes, and a `v3` write never byte-matches a `v2` row
of the same plaintext. So an app moves to `v3` in this order and no other:

1. Deploy a release whose encryptor can read `v3` (this one), still writing `v2`.
2. Run the rotation sweep with `ENCRYPTION_FORMAT=v3` set for the migration
   only, or with `encryptor: FieldEncryptor.fromEnv().withFormat('v3')`.
   Every row is rewritten as `v3` under the current key; the report counts
   them as rewritten.
3. Set `ENCRYPTION_FORMAT=v3` for the service and deploy. From here on every
   write is stamped, and dropping a key is a count check: zero rows stamped
   with it.

Skipping step 2 makes lookups on encrypted columns miss until the sweep runs.

## Reading the Report

The sweep logs one line per table:

```
[key-rotation] user: scanned 99 rows, 12 values rewritten (from c3d4e5f6a7b8: 12), 403 already current, 188 plaintext, 3 unreadable (rows 214d3262-…)
```

| Field | Meaning |
| --- | --- |
| rewritten (from &lt;id&gt;: n) | Values that were under a previous key, now under the current one. The id is the fingerprint of the key they came from. |
| already current | Nothing to do. |
| plaintext | Not a ciphertext (empty strings, columns classified after they were populated). Left alone. |
| unreadable (rows …) | No key in the ring opens them under any tenant. The row ids are the audit trail. |

Zero rewritten and zero unreadable means the old key can be dropped.
Unreadable values mean a key is missing from the ring; find it before
dropping anything (see Recovery). A value that stays unreadable after every
known key is in the ring was written under a key nobody kept; the only fix
is re-entering it.

Tenants: keys are derived per tenant, so a value only opens under the
tenant it was written with. The sweep tries a row's own hints first
(`organization_id`, then the empty tenant) and falls back to every
organization the database knows. Entities with another shape (an
organization being its own tenant, IAM style) pass `tenantIdsFor`.

## Recovery: A Key Was Replaced Under Live Data

Symptoms: rows from a date range unreadable, sign-in 500 on JWKS decrypt,
deploy configuration "could not be resolved", `DecryptionError` bursts
starting at a deploy or stack re-creation.

1. **Establish the timeline.** When did the key change, and what was live
   before? Deploy history, ECS task definition revisions, SSM parameter
   versions and Pulumi checkpoint timestamps all carry dates. Data written
   inside each window is under that window's key.
2. **Recover every key that was ever live.** A generated key that was
   deleted from SSM still exists, encrypted under the deploy passphrase,
   inside Pulumi's checkpoint history. Decrypt a checkpoint from each window
   into a scratch local stack and pull the `<service>-encryption-key`
   parameter values; the platform repo carries
   `scripts/set-legacy-encryption-keys-from-checkpoints.sh` which does this
   and hands the union to `config set` without printing a key. If a key
   truly cannot be found, the data under it is gone; say so plainly.
3. **Widen the ring with all of them.** Set `LEGACY_ENCRYPTION_KEYS` at
   application scope so every service gets the whole set; a key that never
   matches a service's data costs microseconds and is otherwise harmless.
4. **Sweep every service that holds encrypted columns**, not only the one
   that showed symptoms. Every service was on the same infrastructure and
   had its own key replaced. The report tells you what each lost.
5. **Deploy once more.** The deployer reads stored configuration before the
   sweep ran, so the release that carried the sweep may have gone out with
   config it could not read. The next deploy sees everything.
6. **Narrow the ring** when every report is clean.

Double-wrapped values: platform-management's environment-variable tables
encrypt with the module's own `EncryptionService` (`iv:tag:data`, keyed by
sha256 of the same `ENCRYPTION_KEY`) before MikroORM wraps the result with
`FieldEncryptor`. A sweep there must open both layers; the platform's
per-module copy of the sweep (`reencrypt-legacy-key.util.ts`) does. The
framework sweep handles the outer layer only.

Better-auth's JWKS private key is encrypted with `BETTER_AUTH_SECRET`, not
`ENCRYPTION_KEY`. If that secret changed, delete the `jwks` rows and let
better-auth mint a new pair on the next token request; sessions survive,
tokens signed with the old key (15-minute lifetime) stop verifying.

## What to Avoid

- **Infrastructure that regenerates a key.** A key created by a deploy must
  be protected from replacement and its value ignored on later updates.
  Otherwise a stack re-creation silently strands every row under it.
- **Dropping a key before the sweep is clean.** There is no undo.
- **Rotating via `config push` of a whole scope.** Push is authoritative for
  the scope; anything missing from the file is cleared. Use `config set` for
  one variable.
- **Rewriting through entities.** The sweep uses raw SQL on purpose; loading
  entities decrypts on hydrate and re-encrypts on flush, which is blind to
  which key opened the value and double-encrypts already-encrypted input.
- **Relying on equality lookups mid-rotation.** Rows under a previous key do
  not match a `WHERE` on the column until rewritten. Sweep promptly.

## Summary

The service can hold several keys at once: one it writes with and any it
still needs to read with. Changing a key safely means adding the old one to
the read list, deploying, running the sweep migration that rewrites old rows,
and only then removing the old key. If a key was replaced by accident, the
same machinery recovers the data as long as the old key can still be found,
which for platform-generated keys means Pulumi's checkpoint history. What the
sweep reports as unreadable after every known key is in the ring is the
honest measure of what was lost.

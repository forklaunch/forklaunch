# Encryption key rotation

Compliance-classified columns (`pii`, `phi`, `pci`) are encrypted at rest with
AES-256-GCM under a key derived from your service's `ENCRYPTION_KEY`. A value
can only ever be opened with the key it was written under, so changing that
variable on its own makes every existing row unreadable. This page is how to
change it without that happening.

## The key ring

`FieldEncryptor` holds a ring: one **current** key and any number of
**previous** keys.

- Every write uses the current key.
- Every read tries the current key first, then each previous key in order.
  AES-GCM's authentication tag tells the encryptor which key is right, so a
  wrong key is rejected rather than producing garbage.

The ring comes from the environment:

| Variable | Meaning |
| --- | --- |
| `ENCRYPTION_KEY` | The current key. New values are written with it. |
| `LEGACY_ENCRYPTION_KEYS` | Every previous key, comma separated. Read-only. |

```ts
import { FieldEncryptor } from '@forklaunch/core/persistence';

// Explicit
const encryptor = new FieldEncryptor(process.env.ENCRYPTION_KEY!, {
  previousKeys: ['the-key-before-this-one']
});

// Or straight from the environment
const encryptor = FieldEncryptor.fromEnv();
```

`encryptor.keyIds` returns short fingerprints of every key in the ring. They
are safe to log and reveal nothing about the keys.

## Rotating a key

1. **Add the old key to the ring.** Set `LEGACY_ENCRYPTION_KEYS` to the
   current value of `ENCRYPTION_KEY` (append it if the list is not empty), then
   set `ENCRYPTION_KEY` to the new key. Deploy. Nothing breaks: old rows open
   with the previous key, new rows are written with the new one.
2. **Run the sweep.** Add a migration that rewrites everything still under a
   previous key:

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
       const execute = (sql: string, params?: unknown[]) =>
         this.execute(sql, params);
       const reports = await reencryptEncryptedColumns({
         em: this.getEntityManager(),
         execute,
         encryptor: FieldEncryptor.fromEnv(),
         fallbackTenantIds: await collectFallbackTenantIds(execute),
         log: (m) => console.info(m)
       });
       const totals = rotationTotals(reports);
       if (totals.unreadable > 0) {
         throw new Error(
           `${totals.unreadable} value(s) open with no key in the ring; see the log for row ids`
         );
       }
     }
     override async down(): Promise<void> {}
   }
   ```

   The sweep is safe to re-run. It logs one line per table: how many values
   were rewritten (and from which key fingerprint), how many were already
   current, and the ids of any rows that open with no key at all.
3. **Drop the old key.** Once the sweep reports nothing rewritten and nothing
   unreadable, remove the old key from `LEGACY_ENCRYPTION_KEYS`.

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

## Tenants

Keys are derived per tenant, so a value only opens under the tenant it was
written with. The sweep tries each row under its likely tenants first (its
`organization_id`, then the empty tenant) and only then under every
organization the database knows. If your entities use another tenant shape
(an organization being its own tenant, say), pass `tenantIdsFor` to extend the
candidates for a row.

## What to avoid

- Never let infrastructure regenerate `ENCRYPTION_KEY`. A key that is created
  by a deploy must be protected from replacement, or every row written under
  it is lost the moment the deploy re-creates it.
- Never remove a key from `LEGACY_ENCRYPTION_KEYS` before the sweep reports
  zero values under it.
- Deterministic encryption means equality lookups compare ciphertexts. Rows
  still under a previous key will not match a `WHERE` on the column until the
  sweep rewrites them, so run the sweep promptly after step 1.

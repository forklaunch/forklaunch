# Authenticated envelopes

`EnvelopeKey` encrypts bytes with AES-256-GCM and a random 96-bit nonce. A required
`EnvelopeProfile` supplies the application wire namespace; tenant UUID, resource
UUID and purpose bind ciphertext to its intended use. No storage, role, env-name,
HTTP or notes policy is embedded here.

`new(profile, key)` preserves unkeyed v1 envelopes. `with_rotation` writes v2
with an authenticated explicit key ID and reads at most four retained key IDs
plus an optional original v1 key. Unknown IDs fail closed. Key material is
private, zeroized on drop and implements neither Debug nor Serde. Application
configuration owns key loading, rotation rollout and retained-data migration.

Fieldnotes explicitly selects the `fieldnotes` namespace. Fixed Node/Rust v1/v2
vectors, rotation, tampering and cross-namespace tests preserve that wire format.

New environment-configured writes use `v3.<key-id>.<nonce>.<ciphertext-and-tag>`. AES-256-GCM uses a tenant-derived HKDF-SHA256 key: salt `forklaunch-envelope`, info `<namespace>:v3:<tenant-uuid>`. All previous AAD bindings remain. v1/v2 reads retain their original derivation; versions are authenticated and cannot be relabeled. Upgrade readers before writers during rolling adoption. Node and Rust fixed vectors cover all three versions.

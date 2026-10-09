# Fresh IAM encryption policy: iam-service-v1

A person can sign in before joining an organization and can belong to several organizations. Their login credential belongs to this application's identity service. It must not change encryption keys when the person switches organizations. The identity service's signing keys have the same service owner.

New scaffolds configure the adapter operation-context hook and retain the HTTP handler scope, including transactions and later session flushes, to the explicit nonempty namespace `iam:identity-service:v1`. Account secrets and JWKS private keys use that namespace with the application's own encryption master key. Each application must have its own securely provisioned key. This is not a default for missing business tenants. Other services keep their existing tenant-bound entity managers; organization access still requires authorization.

The adapter hook also covers direct `BetterAuth.api` and background authentication calls; key choice and the policy marker stay in the application. The generic adapter must not choose a business tenant or silently supply a key. This change depends on the separately reviewed adapter operation-context release (planned 0.5.11); do not merge/publish the scaffold until that version exists and fresh emitted scaffolds have been validated.

Account and JWKS rows carry `encryptionPolicy: 'iam-service-v1'`. The auth ORM wrapper checks that plaintext marker before hydrating secrets, stamps new records, and refuses policy changes through normal updates. Empty, unknown, and legacy markers fail with a migration error. Checks are bounded to 1,000 matching rows. Development account seeds use the same policy. The old `encryptionContext.util.ts` remains unchanged for existing custom integrations; new registrations use the new utility explicitly.

## Existing applications

Do not copy this helper over an existing application's encryption wiring as an automatic upgrade. Older records may use organization IDs, a former empty namespace, or `_seed`; substituting a new namespace would make them unreadable. A reviewed migration must:

1. Inventory existing identity records and the exact old key/context that wrote each one.
2. Retain a tested backup and run under an isolated maintenance identity.
3. Read each old encrypted value under its confirmed old policy and re-encrypt with the new explicit identity policy. Never guess keys or fall back silently.
4. Set the version marker only after successful re-encryption, verify credential/session behavior, and preserve organization membership/authorization.
5. Test rollback before enabling the new handler. The framework's normal empty-tenant rejection stays enabled; any legacy migration requires a separate reviewed offline implementation.

No automatic ciphertext migration or production rollout is included here. Version marking is not access control: Better Auth still authenticates users and checks sessions, and the application still authorizes organization/business data access.

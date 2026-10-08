# Organization-bound generated API access

An organization creator is an owner. For example, someone creating a booking app can call its protected booking endpoints immediately; a viewer invited to that organization cannot. Owning a different organization does not upgrade the viewer's access.

Fresh generated router controllers use `APPLICATION_ADMIN_ROLES` (owner or admin). Platform/system role sets remain unchanged. Better Auth's built-in organization permission statements are composed with the application's custom permissions, so owners can invite members and manage their own organization through the authenticated API.

Every Better Auth role/permission lookup requires both the verified token subject and its `activeOrganizationId`. The IAM service reads that exact membership, never a user's latest active session. The former user-only authorization cache is deliberately bypassed: removing membership takes effect on the next request. This adds an IAM lookup per authorization check. Failed lookups deny access and do not log signed SDK request details.

## Upgrade compatibility

Deploy IAM and its generated consumers together. Internal HMAC routes now include `/user/:id/organizations/:organizationId/surface-roles` and `surface-permissions`; old unscoped routes are removed and fail closed. The Better Auth surfacing helper is now a separate file, leaving base IAM's contract unchanged. No existing memberships, roles, databases, or running applications are automatically migrated. Existing customized controllers need explicit review before adopting the owner/admin guard; billing and platform-specific policies are not globally widened.

Focused tests cover exact membership lookup, owner versus platform authority, revoked membership, and request-signature secrecy. The companion platform acceptance fixture compiles an owned scaffold with the candidate templates and exercises real IAM/booking HTTP APIs against disposable PostgreSQL and Redis. It marks only synthetic email addresses verified locally; email delivery, live deployment, and publication of a new CLI remain separate checks.

---
'@forklaunch/core': minor
---

- **Default audit trail.** Every request handled through ForkLaunch appends one audit record (`log.type: audit`) when its response finishes: user, tenant, method, route, status, duration and a SHA-256 of the body (never the body itself). Refused requests are recorded as `auth_failure` (401), `rbac_deny` (403) or `rate_limit` (429). Every `.deanon` read of a compliant field adds a `data_access` record (field, level, tenant), and every create, update or delete of a record holding pii, phi or pci fields adds a `data_change` record (entity and id, action, sensitive field names, tenant). Entities defined with `defineComplianceEntity` get the change hooks automatically. Set `FORKLAUNCH_AUDIT=off` to turn it off.
- `onComplianceChange(listener)` reports changes to sensitive records, for apps that want their own audit sink.

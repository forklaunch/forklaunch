/**
 * Compile-time type tests for AccessAuth discriminated union.
 *
 * Verifies that the type system enforces:
 * - public routes cannot have auth
 * - authenticated routes cannot have RBAC fields
 * - protected routes must have at least one RBAC group
 * - protected routes pair surfacing functions with their declarations
 * - internal routes must use HMAC
 * - any combination of RBAC groups is valid on protected routes
 */

import { SchemaValidator, string } from '@forklaunch/validator/typebox';
import type { PathParamHttpContractDetails } from '../src/http/types/contractDetails.types';

type SV = ReturnType<typeof SchemaValidator>;

// ---------------------------------------------------------------------------
// ✅ public — no auth required
// ---------------------------------------------------------------------------
const publicRoute: PathParamHttpContractDetails<SV, 'PublicRoute', '/health'> =
  {
    name: 'PublicRoute',
    access: 'public',
    summary: 'Health check',
    responses: { 200: string }
  };

// ---------------------------------------------------------------------------
// ✅ authenticated — JWT, no RBAC
// ---------------------------------------------------------------------------
const authenticatedRoute: PathParamHttpContractDetails<SV, 'AuthRoute', '/me'> =
  {
    name: 'AuthRoute',
    access: 'authenticated',
    summary: 'Get current user',
    auth: {
      jwt: { signatureKey: 'secret' }
    },
    responses: { 200: string }
  };

// ---------------------------------------------------------------------------
// ✅ protected — JWT with allowedRoles + surfaceRoles
// ---------------------------------------------------------------------------
const protectedWithRoles: PathParamHttpContractDetails<
  SV,
  'RolesRoute',
  '/admin'
> = {
  name: 'RolesRoute',
  access: 'protected',
  summary: 'Admin only',
  auth: {
    jwt: { signatureKey: 'secret' },
    allowedRoles: new Set(['admin']),
    surfaceRoles: () => new Set(['admin'])
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ✅ protected — JWT with allowedPermissions + surfacePermissions
// ---------------------------------------------------------------------------
const protectedWithPerms: PathParamHttpContractDetails<
  SV,
  'PermsRoute',
  '/data'
> = {
  name: 'PermsRoute',
  access: 'protected',
  summary: 'Permissioned data',
  auth: {
    jwt: { signatureKey: 'secret' },
    allowedPermissions: new Set(['read:data']),
    surfacePermissions: () => new Set(['read:data'])
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ✅ protected — JWT with requiredScope + surfaceScopes
// ---------------------------------------------------------------------------
const protectedWithScope: PathParamHttpContractDetails<
  SV,
  'ScopeRoute',
  '/scoped'
> = {
  name: 'ScopeRoute',
  access: 'protected',
  summary: 'Scoped access',
  auth: {
    jwt: { signatureKey: 'secret' },
    requiredScope: 'read:all',
    surfaceScopes: () => new Set(['read:all'])
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ✅ protected — combination: roles + permissions
// ---------------------------------------------------------------------------
const protectedCombo: PathParamHttpContractDetails<SV, 'ComboRoute', '/combo'> =
  {
    name: 'ComboRoute',
    access: 'protected',
    summary: 'Roles and permissions',
    auth: {
      jwt: { signatureKey: 'secret' },
      allowedRoles: new Set(['admin']),
      surfaceRoles: () => new Set(['admin']),
      allowedPermissions: new Set(['write']),
      surfacePermissions: () => new Set(['write'])
    },
    responses: { 200: string }
  };

// ---------------------------------------------------------------------------
// ✅ protected — combination: roles + scope
// ---------------------------------------------------------------------------
const protectedRolesAndScope: PathParamHttpContractDetails<
  SV,
  'RoleScopeRoute',
  '/rs'
> = {
  name: 'RoleScopeRoute',
  access: 'protected',
  summary: 'Roles and scope',
  auth: {
    jwt: { signatureKey: 'secret' },
    forbiddenRoles: new Set(['banned']),
    surfaceRoles: () => new Set([]),
    requiredScope: 'admin',
    surfaceScopes: () => new Set(['admin'])
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ✅ protected — combination: all three groups
// ---------------------------------------------------------------------------
const protectedAll: PathParamHttpContractDetails<SV, 'AllRoute', '/all'> = {
  name: 'AllRoute',
  access: 'protected',
  summary: 'All RBAC groups',
  auth: {
    jwt: { signatureKey: 'secret' },
    allowedPermissions: new Set(['read']),
    surfacePermissions: () => new Set(['read']),
    forbiddenRoles: new Set(['banned']),
    surfaceRoles: () => new Set([]),
    requiredScope: 'full',
    surfaceScopes: () => new Set(['full'])
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ✅ internal — HMAC
// ---------------------------------------------------------------------------
const internalRoute: PathParamHttpContractDetails<
  SV,
  'InternalRoute',
  '/sync'
> = {
  name: 'InternalRoute',
  access: 'internal',
  summary: 'Internal sync',
  auth: {
    hmac: { secretKeys: { default: 'key' } }
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ❌ public — cannot have auth
// ---------------------------------------------------------------------------
// @ts-expect-error — public routes cannot have auth
const publicWithAuth: PathParamHttpContractDetails<SV, 'Bad', '/bad'> = {
  name: 'Bad',
  access: 'public',
  summary: 'Should fail',
  auth: {
    jwt: { signatureKey: 'secret' }
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ❌ authenticated — cannot have allowedRoles
// ---------------------------------------------------------------------------
// @ts-expect-error — authenticated routes cannot have RBAC fields
const authenticatedWithRoles: PathParamHttpContractDetails<SV, 'Bad', '/bad'> =
  {
    name: 'Bad',
    access: 'authenticated',
    summary: 'Should fail',
    auth: {
      jwt: { signatureKey: 'secret' },
      allowedRoles: new Set(['admin'])
    },
    responses: { 200: string }
  };

// ---------------------------------------------------------------------------
// ❌ authenticated — cannot have allowedPermissions
// ---------------------------------------------------------------------------
// @ts-expect-error — authenticated routes cannot have RBAC fields
const authenticatedWithPerms: PathParamHttpContractDetails<SV, 'Bad', '/bad'> =
  {
    name: 'Bad',
    access: 'authenticated',
    summary: 'Should fail',
    auth: {
      jwt: { signatureKey: 'secret' },
      allowedPermissions: new Set(['read'])
    },
    responses: { 200: string }
  };

// ---------------------------------------------------------------------------
// ✅ protected — allowedPermissions without surfacePermissions
//    (surfacePermissions can be provided at router/app level)
// ---------------------------------------------------------------------------
const protectedMissingSurface: PathParamHttpContractDetails<
  SV,
  'PermsNoSurface',
  '/perms'
> = {
  name: 'PermsNoSurface',
  access: 'protected',
  summary: 'Surfacing at router level',
  auth: {
    jwt: { signatureKey: 'secret' },
    allowedPermissions: new Set(['read'])
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ✅ protected — allowedRoles without surfaceRoles
//    (surfaceRoles can be provided at router/app level)
// ---------------------------------------------------------------------------
const protectedMissingSurfaceRoles: PathParamHttpContractDetails<
  SV,
  'RolesNoSurface',
  '/roles'
> = {
  name: 'RolesNoSurface',
  access: 'protected',
  summary: 'Surfacing at router level',
  auth: {
    jwt: { signatureKey: 'secret' },
    allowedRoles: new Set(['admin'])
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ✅ protected — requiredScope without surfaceScopes
//    (surfaceScopes can be provided at router/app level)
// ---------------------------------------------------------------------------
const protectedMissingSurfaceScopes: PathParamHttpContractDetails<
  SV,
  'ScopeNoSurface',
  '/scoped'
> = {
  name: 'ScopeNoSurface',
  access: 'protected',
  summary: 'Surfacing at router level',
  auth: {
    jwt: { signatureKey: 'secret' },
    requiredScope: 'read:all'
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ❌ protected — no RBAC at all (empty auth)
// ---------------------------------------------------------------------------
// @ts-expect-error — protected routes must have at least one RBAC group
const protectedNoRbac: PathParamHttpContractDetails<SV, 'Bad', '/bad'> = {
  name: 'Bad',
  access: 'protected',
  summary: 'Should fail',
  auth: {
    jwt: { signatureKey: 'secret' }
  },
  responses: { 200: string }
};

// ---------------------------------------------------------------------------
// ❌ internal — cannot use JWT (must be HMAC)
// ---------------------------------------------------------------------------
// @ts-expect-error — internal routes must use HMAC
const internalWithJwt: PathParamHttpContractDetails<SV, 'Bad', '/bad'> = {
  name: 'Bad',
  access: 'internal',
  summary: 'Should fail',
  auth: {
    jwt: { signatureKey: 'secret' }
  },
  responses: { 200: string }
};

// Prevent unused variable warnings
void publicRoute;
void authenticatedRoute;
void protectedWithRoles;
void protectedWithPerms;
void protectedWithScope;
void protectedCombo;
void protectedRolesAndScope;
void protectedAll;
void internalRoute;
void publicWithAuth;
void authenticatedWithRoles;
void authenticatedWithPerms;
void protectedMissingSurface;
void protectedMissingSurfaceRoles;
void protectedMissingSurfaceScopes;
void protectedNoRbac;
void internalWithJwt;

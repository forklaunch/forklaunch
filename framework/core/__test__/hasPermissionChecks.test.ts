import { describe, expect, it } from 'vitest';
import { hasPermissionChecks } from '../src/http/guards/hasPermissionChecks';

describe('hasPermissionChecks', () => {
  describe('should return true', () => {
    it('when object has allowedPermissions', () => {
      const auth = {
        allowedPermissions: new Set(['read:user', 'write:user'])
      };
      expect(hasPermissionChecks(auth)).toBe(true);
    });

    it('when object has forbiddenPermissions', () => {
      const auth = {
        forbiddenPermissions: new Set(['delete:user'])
      };
      expect(hasPermissionChecks(auth)).toBe(true);
    });

    it('when object has both allowedPermissions and forbiddenPermissions', () => {
      const auth = {
        allowedPermissions: new Set(['read:user']),
        forbiddenPermissions: new Set(['delete:user'])
      };
      expect(hasPermissionChecks(auth)).toBe(true);
    });
  });

  describe('should return false', () => {
    it('when object has neither allowedPermissions nor forbiddenPermissions', () => {
      const auth = {
        requiredScope: 'read:user'
      };
      expect(hasPermissionChecks(auth)).toBe(false);
    });

    it('when input is null', () => {
      expect(hasPermissionChecks(null)).toBe(false);
    });

    it('when input is undefined', () => {
      expect(hasPermissionChecks(undefined)).toBe(false);
    });

    it('when input is a string', () => {
      expect(hasPermissionChecks('permission')).toBe(false);
    });

    it('when input is an empty object', () => {
      expect(hasPermissionChecks({})).toBe(false);
    });

    it('BUG: should return false when allowedPermissions is null', () => {
      const auth = { allowedPermissions: null };
      expect(hasPermissionChecks(auth)).toBe(false);
    });

    it('BUG: should return false when forbiddenPermissions is undefined', () => {
      const auth = { forbiddenPermissions: undefined };
      expect(hasPermissionChecks(auth)).toBe(false);
    });

    it('BUG: should return false when both are null/undefined', () => {
      const auth = { allowedPermissions: null, forbiddenPermissions: undefined };
      expect(hasPermissionChecks(auth)).toBe(false);
    });
  });
});

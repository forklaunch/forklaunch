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

    // The property is present but holds no permissions. The guard once
    // checked only that the key existed, so these returned true (FOR-29).
    it('when allowedPermissions is null', () => {
      const auth = { allowedPermissions: null };
      expect(hasPermissionChecks(auth)).toBe(false);
    });

    it('when forbiddenPermissions is undefined', () => {
      const auth = { forbiddenPermissions: undefined };
      expect(hasPermissionChecks(auth)).toBe(false);
    });

    it('when allowedPermissions is null and forbiddenPermissions is undefined', () => {
      const auth = {
        allowedPermissions: null,
        forbiddenPermissions: undefined
      };
      expect(hasPermissionChecks(auth)).toBe(false);
    });

    it('when allowedPermissions is an empty Set', () => {
      expect(hasPermissionChecks({ allowedPermissions: new Set() })).toBe(
        false
      );
    });

    it('when forbiddenPermissions is an empty Set', () => {
      expect(hasPermissionChecks({ forbiddenPermissions: new Set() })).toBe(
        false
      );
    });
  });

  it('returns true when one set is empty and the other is not', () => {
    const auth = {
      allowedPermissions: new Set<string>(),
      forbiddenPermissions: new Set(['delete:user'])
    };
    expect(hasPermissionChecks(auth)).toBe(true);
  });
});

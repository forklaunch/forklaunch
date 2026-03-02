import { describe, expect, it } from 'vitest';
import { hasRoleChecks } from '../src/http/guards/hasRoleChecks';

describe('hasRoleChecks', () => {
  describe('should return true', () => {
    it('when object has allowedRoles', () => {
      const auth = {
        allowedRoles: new Set(['admin', 'user'])
      };
      expect(hasRoleChecks(auth)).toBe(true);
    });

    it('when object has forbiddenRoles', () => {
      const auth = {
        forbiddenRoles: new Set(['guest'])
      };
      expect(hasRoleChecks(auth)).toBe(true);
    });

    it('when object has both allowedRoles and forbiddenRoles', () => {
      const auth = {
        allowedRoles: new Set(['admin']),
        forbiddenRoles: new Set(['guest'])
      };
      expect(hasRoleChecks(auth)).toBe(true);
    });
  });

  describe('should return false', () => {
    it('when object has neither allowedRoles nor forbiddenRoles', () => {
      const auth = {
        requiredScope: 'read:user'
      };
      expect(hasRoleChecks(auth)).toBe(false);
    });

    it('when input is null', () => {
      expect(hasRoleChecks(null)).toBe(false);
    });

    it('when input is undefined', () => {
      expect(hasRoleChecks(undefined)).toBe(false);
    });

    it('when input is a string', () => {
      expect(hasRoleChecks('role')).toBe(false);
    });

    it('when input is an empty object', () => {
      expect(hasRoleChecks({})).toBe(false);
    });

    it('when allowedRoles is null', () => {
      const auth = {
        allowedRoles: null
      };
      expect(hasRoleChecks(auth)).toBe(false);
    });

    it('when allowedRoles is undefined', () => {
      const auth = {
        allowedRoles: undefined
      };
      expect(hasRoleChecks(auth)).toBe(false);
    });

    it('when forbiddenRoles is null', () => {
      const auth = {
        forbiddenRoles: null
      };
      expect(hasRoleChecks(auth)).toBe(false);
    });

    it('when forbiddenRoles is undefined', () => {
      const auth = {
        forbiddenRoles: undefined
      };
      expect(hasRoleChecks(auth)).toBe(false);
    });

    it('when allowedRoles is an empty Set', () => {
      const auth = {
        allowedRoles: new Set([])
      };
      expect(hasRoleChecks(auth)).toBe(false);
    });

    it('when forbiddenRoles is an empty Set', () => {
      const auth = {
        forbiddenRoles: new Set([])
      };
      expect(hasRoleChecks(auth)).toBe(false);
    });

    it('when allowedRoles is not a Set (array)', () => {
      const auth = {
        allowedRoles: ['admin', 'user']
      };
      expect(hasRoleChecks(auth)).toBe(false);
    });

    it('when forbiddenRoles is not a Set (string)', () => {
      const auth = {
        forbiddenRoles: 'guest'
      };
      expect(hasRoleChecks(auth)).toBe(false);
    });

    it('when both allowedRoles and forbiddenRoles are empty Sets', () => {
      const auth = {
        allowedRoles: new Set([]),
        forbiddenRoles: new Set([])
      };
      expect(hasRoleChecks(auth)).toBe(false);
    });

  });
});

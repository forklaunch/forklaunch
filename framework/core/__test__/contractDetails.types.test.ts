import { describe, expect, it } from 'vitest';
import { hasScopeChecks } from '../src/http/guards/hasScopeChecks';
import { isPathParamHttpContractDetails } from '../src/http/guards/isPathParamContractDetails';

describe('contractDetails.types', () => {
  describe('PathParamHttpContractDetails structure', () => {
    it('should have required fields: name, summary, and responses', () => {
      const contractDetails = {
        name: 'getUser',
        access: 'public',
        summary: 'Get user by ID',
        responses: {
          200: { json: { id: 'string', name: 'string' } }
        }
      };

      expect(isPathParamHttpContractDetails(contractDetails)).toBe(true);
      expect(contractDetails.name).toBe('getUser');
      expect(contractDetails.summary).toBe('Get user by ID');
      expect(contractDetails.responses).toBeDefined();
    });

    it('should accept optional params field', () => {
      const contractDetails = {
        name: 'getUser',
        access: 'public',
        summary: 'Get user by ID',
        params: { userId: 'string' },
        responses: {
          200: { json: { id: 'string' } }
        }
      };

      expect(isPathParamHttpContractDetails(contractDetails)).toBe(true);
      expect(contractDetails.params).toBeDefined();
    });

    it('should accept optional query field', () => {
      const contractDetails = {
        name: 'getUser',
        access: 'public',
        summary: 'Get user by ID',
        query: { include: 'string' },
        responses: {
          200: { json: { id: 'string' } }
        }
      };

      expect(isPathParamHttpContractDetails(contractDetails)).toBe(true);
      expect(contractDetails.query).toBeDefined();
    });
  });

  describe('SchemaAuthMethods structure', () => {
    it('should support requiredScope in auth object', () => {
      const auth = {
        requiredScope: 'read:user',
        jwt: { signatureKey: 'secret' }
      };

      expect(hasScopeChecks(auth)).toBe(true);
      expect(auth.requiredScope).toBe('read:user');
    });

    it('should support optional surfaceScopes in auth object', () => {
      const auth = {
        requiredScope: 'read:user',
        surfaceScopes: () => 'admin',
        jwt: { signatureKey: 'secret' }
      };

      expect(hasScopeChecks(auth)).toBe(true);
      expect(auth.requiredScope).toBe('read:user');
      expect(typeof auth.surfaceScopes).toBe('function');
    });

    it('should support scopeHeirarchy in auth object', () => {
      const auth = {
        requiredScope: 'read:user',
        scopeHeirarchy: ['read:user', 'write:user', 'admin'],
        jwt: { signatureKey: 'secret' }
      };

      expect(hasScopeChecks(auth)).toBe(true);
      expect(Array.isArray(auth.scopeHeirarchy)).toBe(true);
    });

    it('should support sessionSchema in auth object', () => {
      const auth = {
        requiredScope: 'read:user',
        sessionSchema: { userId: 'string', role: 'string' },
        jwt: { signatureKey: 'secret' }
      };

      expect(hasScopeChecks(auth)).toBe(true);
      expect(auth.sessionSchema).toBeDefined();
    });
  });

  describe('Auth methods compatibility', () => {
    it('should support JWT auth method', () => {
      const auth = {
        requiredScope: 'read:user',
        jwt: { signatureKey: 'secret-key' }
      };

      expect(hasScopeChecks(auth)).toBe(true);
      expect(auth.jwt).toBeDefined();
    });

    it('should support basic auth method', () => {
      const auth = {
        requiredScope: 'read:user',
        basic: {
          login: (username: string) => username === 'admin'
        }
      };

      expect(hasScopeChecks(auth)).toBe(true);
      expect(auth.basic).toBeDefined();
      expect(typeof auth.basic.login).toBe('function');
    });

    it('should support HMAC auth method', () => {
      const auth = {
        hmac: {
          secretKeys: { key1: 'secret1', key2: 'secret2' }
        }
      };

      // HMAC doesn't require requiredScope
      expect(auth.hmac).toBeDefined();
      expect(typeof auth.hmac.secretKeys).toBe('object');
    });
  });

  describe('Contract details with auth', () => {
    it('should support contract details with auth configuration', () => {
      const contractDetails = {
        name: 'getUser',
        access: 'protected',
        summary: 'Get user by ID',
        responses: {
          200: { json: { id: 'string' } }
        },
        auth: {
          requiredScope: 'read:user',
          jwt: { signatureKey: 'secret' }
        }
      };

      expect(isPathParamHttpContractDetails(contractDetails)).toBe(true);
      expect(contractDetails.auth).toBeDefined();
      if (contractDetails.auth) {
        expect(hasScopeChecks(contractDetails.auth)).toBe(true);
      }
    });
  });
});

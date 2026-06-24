import { beforeEach, describe, expect, it } from 'vitest';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';
import {
  clearComplianceRegistries,
  getEntityComplianceFields,
  getEntityUserIdField
} from '../src/persistence/complianceTypes';

/**
 * Unit tests for compliance entity registration and discovery.
 *
 * This test verifies the bug report claim that "ComplianceDataService cannot
 * discover compliance entities defined using defineComplianceEntity".
 *
 * The bug report from forklaunch-platform states:
 * - Entities ARE defined using defineComplianceEntity with .compliance('pii')
 * - Entities ARE registered with MikroORM (appear in metadata)
 * - BUT ComplianceDataService cannot find compliance annotations
 *
 * These tests verify that the framework registry correctly stores and retrieves
 * compliance metadata when entities are defined.
 */
describe('ComplianceDataService entity discovery', () => {
  beforeEach(() => {
    // Clear global registries before each test to prevent cross-test pollution
    clearComplianceRegistries();
  });
  it('defineComplianceEntity registers entities in compliance registry', () => {
    // Define entities exactly as they would be in forklaunch-platform
    defineComplianceEntity({
      name: 'DiscoveryTestUser',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        email: fp.string().unique().compliance('pii'),
        name: fp.string().compliance('pii'),
        phoneNumber: fp.string().nullable().compliance('pii'),
        createdAt: fp.datetime().compliance('none'),
        updatedAt: fp.datetime().compliance('none')
      },
      userIdField: 'id'
    });

    defineComplianceEntity({
      name: 'DiscoveryTestAccount',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        userId: fp.uuid().compliance('none'),
        provider: fp.string().compliance('none'),
        accessToken: fp.string().nullable().compliance('pii'),
        refreshToken: fp.string().nullable().compliance('pii'),
        createdAt: fp.datetime().compliance('none'),
        updatedAt: fp.datetime().compliance('none')
      },
      userIdField: 'userId'
    });

    // Verify that getEntityComplianceFields (used by ComplianceDataService)
    // can discover these entities
    const userFields = getEntityComplianceFields('DiscoveryTestUser');
    const accountFields = getEntityComplianceFields('DiscoveryTestAccount');

    // These assertions match the debug logging in the bug report that showed
    // complianceEntities: [] when it should have contained entities
    expect(userFields).toBeDefined();
    expect(accountFields).toBeDefined();

    // Verify PII fields are discoverable
    expect(userFields?.get('email')).toBe('pii');
    expect(userFields?.get('name')).toBe('pii');
    expect(userFields?.get('phoneNumber')).toBe('pii');
    expect(accountFields?.get('accessToken')).toBe('pii');
    expect(accountFields?.get('refreshToken')).toBe('pii');

    // Verify non-PII fields are also registered
    expect(userFields?.get('id')).toBe('none');
    expect(userFields?.get('createdAt')).toBe('none');
    expect(accountFields?.get('provider')).toBe('none');
  });

  it('getEntityComplianceFields returns undefined for unregistered entities', () => {
    // This mimics the bug report scenario where entities appear in ORM metadata
    // but not in compliance registry
    const fields = getEntityComplianceFields('NonExistentEntity');
    expect(fields).toBeUndefined();
  });

  it('userIdField is registered and discoverable', () => {
    defineComplianceEntity({
      name: 'UserIdTestEntity',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        customUserId: fp.uuid().compliance('none'),
        data: fp.string().compliance('pii')
      },
      userIdField: 'customUserId'
    });

    // ComplianceDataService uses getEntityUserIdField to resolve the user link
    const userIdField = getEntityUserIdField('UserIdTestEntity');
    expect(userIdField).toBe('customUserId');
  });

  it('userIdField defaults to userId when not specified', () => {
    defineComplianceEntity({
      name: 'DefaultUserIdEntity',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        userId: fp.uuid().compliance('none'),
        data: fp.string().compliance('pii')
      }
      // No userIdField specified
    });

    const userIdField = getEntityUserIdField('DefaultUserIdEntity');
    expect(userIdField).toBe('userId');
  });

  it('entities with only PII fields are discoverable', () => {
    defineComplianceEntity({
      name: 'PiiOnlyEntity',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        secret: fp.string().compliance('pii'),
        token: fp.string().compliance('pii')
      }
    });

    const fields = getEntityComplianceFields('PiiOnlyEntity');
    expect(fields).toBeDefined();

    // ComplianceDataService checks if any fields are pii/phi/pci
    const hasPii = [...(fields?.values() ?? [])].some(
      (level) => level === 'pii' || level === 'phi' || level === 'pci'
    );
    expect(hasPii).toBe(true);
  });

  it('entities with no PII are still registered but filtered by service', () => {
    defineComplianceEntity({
      name: 'NoPiiEntity',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        data: fp.string().compliance('none'),
        count: fp.integer().compliance('none')
      }
    });

    const fields = getEntityComplianceFields('NoPiiEntity');
    expect(fields).toBeDefined();

    // ComplianceDataService would skip this entity because it has no PII
    const hasPii = [...(fields?.values() ?? [])].some(
      (level) => level === 'pii' || level === 'phi' || level === 'pci'
    );
    expect(hasPii).toBe(false);
  });

  it('multiple entities can be registered and discovered independently', () => {
    // Simulate the IAM module from forklaunch-platform with multiple entities
    const entities = [
      'MultiTestUser',
      'MultiTestAccount',
      'MultiTestSession',
      'MultiTestOrganization'
    ];

    entities.forEach((name) => {
      defineComplianceEntity({
        name,
        properties: {
          id: fp.uuid().primary().compliance('none'),
          piiField: fp.string().compliance('pii'),
          sequence: fp.integer().compliance('none')
        }
      });
    });

    // All entities should be independently discoverable
    entities.forEach((name) => {
      const fields = getEntityComplianceFields(name);
      expect(fields).toBeDefined();
      expect(fields?.get('piiField')).toBe('pii');
      expect(fields?.get('sequence')).toBe('none');
    });
  });
});

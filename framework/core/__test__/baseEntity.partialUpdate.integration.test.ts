/**
 * Integration tests for partial update functionality using em.assign()
 *
 * These tests validate that partial updates via em.findOneOrFail + em.assign
 * preserve all fields that are not included in the update DTO.
 */

import { EntityManager } from '@mikro-orm/core';
import { v4 } from 'uuid';
import { describe, expect, it, vi } from 'vitest';

describe('Partial Update Tests (em.assign pattern)', () => {
  function createMockEntity(overrides: Record<string, unknown> = {}) {
    return {
      id: v4(),
      email: 'john.doe@example.com',
      firstName: 'John',
      lastName: 'Doe',
      phoneNumber: '+1234567890',
      bio: 'Software engineer',
      createdAt: new Date('2024-01-01'),
      updatedAt: new Date('2024-01-01'),
      ...overrides
    };
  }

  function createMockEm(existingEntity: Record<string, unknown>) {
    return {
      findOneOrFail: vi.fn().mockResolvedValue(existingEntity),
      assign: vi.fn((entity, data) => Object.assign(entity, data)),
      persist: vi.fn().mockReturnValue({ flush: vi.fn() })
    } as unknown as EntityManager;
  }

  // Simulates the inline mapper update pattern:
  // const entity = await em.findOneOrFail(Schema, { id });
  // em.assign(entity, { ...partialData, updatedAt: new Date() });
  // await em.persist(entity).flush();
  async function performUpdate(
    em: EntityManager,
    entitySchema: unknown,
    updateData: Record<string, unknown>
  ) {
    const { id, ...rest } = updateData;
    const entity = await em.findOneOrFail(entitySchema as never, { id });
    em.assign(entity, { ...rest, updatedAt: new Date() });
    return entity;
  }

  const FakeSchema = {} as unknown; // Placeholder for EntitySchema

  it('should preserve all fields when updating only email', async () => {
    const userId = v4();
    const existingUser = createMockEntity({ id: userId });
    const mockEm = createMockEm(existingUser);

    const updatedUser = await performUpdate(mockEm, FakeSchema, {
      id: userId,
      email: 'john.updated@example.com'
    });

    expect(mockEm.findOneOrFail).toHaveBeenCalledWith(FakeSchema, {
      id: userId
    });
    expect((updatedUser as Record<string, unknown>).email).toBe(
      'john.updated@example.com'
    );
    expect((updatedUser as Record<string, unknown>).firstName).toBe('John');
    expect((updatedUser as Record<string, unknown>).lastName).toBe('Doe');
    expect((updatedUser as Record<string, unknown>).phoneNumber).toBe(
      '+1234567890'
    );
    expect((updatedUser as Record<string, unknown>).bio).toBe(
      'Software engineer'
    );
    expect((updatedUser as Record<string, unknown>).createdAt).toEqual(
      new Date('2024-01-01')
    );
  });

  it('should preserve all fields when updating only firstName', async () => {
    const userId = v4();
    const existingUser = createMockEntity({
      id: userId,
      email: 'jane@example.com',
      firstName: 'Jane',
      lastName: 'Smith',
      phoneNumber: '+9876543210',
      bio: 'Product manager'
    });
    const mockEm = createMockEm(existingUser);

    const updatedUser = (await performUpdate(mockEm, FakeSchema, {
      id: userId,
      firstName: 'Janet'
    })) as Record<string, unknown>;

    expect(updatedUser.firstName).toBe('Janet');
    expect(updatedUser.email).toBe('jane@example.com');
    expect(updatedUser.lastName).toBe('Smith');
    expect(updatedUser.phoneNumber).toBe('+9876543210');
    expect(updatedUser.bio).toBe('Product manager');
  });

  it('should handle null values in partial updates', async () => {
    const userId = v4();
    const existingUser = createMockEntity({
      id: userId,
      phoneNumber: '+5555555555',
      bio: 'Test bio'
    });
    const mockEm = createMockEm(existingUser);

    const updatedUser = (await performUpdate(mockEm, FakeSchema, {
      id: userId,
      phoneNumber: null
    })) as Record<string, unknown>;

    expect(updatedUser.phoneNumber).toBeNull();
    expect(updatedUser.email).toBe('john.doe@example.com');
    expect(updatedUser.firstName).toBe('John');
    expect(updatedUser.bio).toBe('Test bio');
  });

  it('should update multiple fields at once while preserving others', async () => {
    const userId = v4();
    const existingUser = createMockEntity({
      id: userId,
      email: 'multi@example.com',
      firstName: 'Multi',
      lastName: 'Field',
      bio: 'Original bio'
    });
    const mockEm = createMockEm(existingUser);

    const updatedUser = (await performUpdate(mockEm, FakeSchema, {
      id: userId,
      email: 'updated.multi@example.com',
      bio: 'Updated bio'
    })) as Record<string, unknown>;

    expect(updatedUser.email).toBe('updated.multi@example.com');
    expect(updatedUser.bio).toBe('Updated bio');
    expect(updatedUser.firstName).toBe('Multi');
    expect(updatedUser.lastName).toBe('Field');
    expect(updatedUser.phoneNumber).toBe('+1234567890');
  });

  it('should not include id in the assign call', async () => {
    const userId = v4();
    const existingUser = createMockEntity({ id: userId });
    const mockEm = createMockEm(existingUser);

    await performUpdate(mockEm, FakeSchema, {
      id: userId,
      email: 'updated@example.com'
    });

    const assignCall = (mockEm.assign as ReturnType<typeof vi.fn>).mock
      .calls[0][1];
    expect(assignCall).not.toHaveProperty('id');
    expect(assignCall).toHaveProperty('email', 'updated@example.com');
    expect(assignCall).toHaveProperty('updatedAt');
  });
});

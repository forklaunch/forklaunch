import { describe, expectTypeOf, it } from 'vitest';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';
import type { ResolvedEntity } from '../src/persistence/mikroOrm.types';

/**
 * A module (say `implementation-iam-base`) declares the minimum shape of the
 * entities it works with. The application that consumes the module defines the
 * real entities, with more columns and different builder options (an id that
 * is generated `onCreate`, timestamps, `.index()`s). The module's service
 * constraints are written as `ResolvedEntity<Shape>`, and the app's entity has
 * to satisfy them.
 *
 * mikro-orm 7.2 declared its property builders invariant (`in out`), so the
 * builder record an inferred entity carries in its `IndexHints` slot no longer
 * unifies across the two definitions. `ResolvedEntity` drops that slot on the
 * entity itself; these assert it drops it on relation targets too, which is
 * where the blueprint IAM module broke.
 */

const shapePermission = defineComplianceEntity({
  name: 'Permission',
  properties: {
    id: fp.string().primary().compliance('none'),
    slug: fp.string().compliance('none')
  }
});

const shapeRole = defineComplianceEntity({
  name: 'Role',
  properties: {
    id: fp.string().primary().compliance('none'),
    permissions: () => fp.manyToMany(shapePermission)
  }
});

const shapeOrganization = defineComplianceEntity({
  name: 'Organization',
  properties: {
    id: fp.string().primary().compliance('none'),
    status: fp.enum().compliance('none')
  }
});

const shapeUser = defineComplianceEntity({
  name: 'User',
  properties: {
    id: fp.string().primary().compliance('none'),
    organization: () => fp.manyToOne(shapeOrganization).nullable()
  }
});

const base = {
  id: fp
    .uuid()
    .primary()
    .onCreate(() => 'generated')
    .compliance('none'),
  createdAt: fp
    .datetime()
    .onCreate(() => new Date())
    .compliance('none')
};

const appPermission = defineComplianceEntity({
  name: 'Permission',
  properties: {
    ...base,
    slug: fp.string().unique().compliance('none')
  }
});

const appRole = defineComplianceEntity({
  name: 'Role',
  properties: {
    ...base,
    name: fp.string().compliance('none'),
    permissions: () => fp.manyToMany(appPermission)
  }
});

enum OrganizationStatus {
  ACTIVE = 'active',
  INACTIVE = 'inactive'
}

const appOrganization = defineComplianceEntity({
  name: 'Organization',
  properties: {
    ...base,
    name: fp.string().compliance('none'),
    status: fp
      .enum(() => OrganizationStatus)
      .default(OrganizationStatus.ACTIVE)
      .compliance('none')
  }
});

const appUser = defineComplianceEntity({
  name: 'User',
  properties: {
    ...base,
    email: fp.string().compliance('pii'),
    organization: () => fp.manyToOne(appOrganization).nullable()
  }
});

type AppPermission = (typeof appPermission)['~entity'];
type AppRole = (typeof appRole)['~entity'];
type AppUser = (typeof appUser)['~entity'];
type AppOrganization = (typeof appOrganization)['~entity'];
type ShapePermission = (typeof shapePermission)['~entity'];
type ShapeRole = (typeof shapeRole)['~entity'];
type ShapeUser = (typeof shapeUser)['~entity'];
type ShapeOrganization = (typeof shapeOrganization)['~entity'];

describe('ResolvedEntity', () => {
  it('lets a richer app entity satisfy a module shape without relations', () => {
    expectTypeOf<AppPermission>().toExtend<ResolvedEntity<ShapePermission>>();
  });

  it('resolves the target of a to-many relation', () => {
    expectTypeOf<AppRole>().toExtend<ResolvedEntity<ShapeRole>>();
    expectTypeOf<ResolvedEntity<ShapeRole>['permissions']>().not.toBeAny();
  });

  it('resolves the target of a to-one relation, nullable included', () => {
    expectTypeOf<AppUser>().toExtend<ResolvedEntity<ShapeUser>>();
    expectTypeOf<ResolvedEntity<ShapeUser>['organization']>().toExtend<
      { id: string } | null | undefined
    >();
  });

  it('passes an untyped enum (`any`) through instead of treating it as an entity', () => {
    expectTypeOf<AppOrganization>().toExtend<
      ResolvedEntity<ShapeOrganization> & { status: OrganizationStatus }
    >();
  });

  it('keeps scalars and the data fields the module reads', () => {
    expectTypeOf<
      ResolvedEntity<ShapePermission>['slug']
    >().toEqualTypeOf<string>();
    expectTypeOf<ResolvedEntity<ShapePermission>>().not.toHaveProperty('name');
  });

  it('still rejects an entity that lacks a field the shape needs', () => {
    expectTypeOf<AppRole>().not.toExtend<ResolvedEntity<ShapePermission>>();
  });
});

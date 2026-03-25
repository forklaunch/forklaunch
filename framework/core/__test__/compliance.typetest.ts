/**
 * Compile-time type tests for fp + defineComplianceEntity.
 */

import type { Collection, InferEntity } from '@mikro-orm/core';
import { p } from '@mikro-orm/core';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';

// ✅ All scalar fields classified, .compliance() last in chain
const User = defineComplianceEntity({
  name: 'User',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    email: fp.string().unique().compliance('pii'),
    medicalRecord: fp.string().nullable().compliance('phi'),
    cardNumber: fp.string().compliance('pci'),
    age: fp.integer().compliance('none'),
    score: fp.double().compliance('none'),
    active: fp.boolean().compliance('none'),
    status: fp.enum(['active', 'inactive'] as const).compliance('none'),
    metadata: fp.json().compliance('none'),
    tags: fp.array().compliance('none')
  }
});

type UserType = InferEntity<typeof User>;
const user: UserType = {
  id: 'uuid',
  email: 'test@test.com',
  medicalRecord: null,
  cardNumber: '4111',
  age: 30,
  score: 9.5,
  active: true,
  status: 'active',
  metadata: { theme: 'dark' },
  tags: ['admin']
};
const _id: string = user.id;
const _email: string = user.email;
const _card: string = user.cardNumber;
const _age: number = user.age;
const _active: boolean = user.active;
const _status: 'active' | 'inactive' = user.status;

// ✅ Relation auto-classified (no .compliance() needed)
const Organization = defineComplianceEntity({
  name: 'Organization',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    name: fp.string().compliance('none'),
    users: () => fp.oneToMany(() => User).mappedBy('organization')
  }
});
type OrgType = InferEntity<typeof Organization>;
const _orgName: string = ({} as OrgType).name;

// ✅ Enum with .array() — enum-specific method
const WithEnumArray = defineComplianceEntity({
  name: 'WithEnumArray',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    tags: fp
      .enum(['a', 'b'] as const)
      .array()
      .compliance('none')
  }
});

// ❌ fp.string() without .compliance()
const Bad1 = defineComplianceEntity({
  name: 'Bad1',
  properties: {
    // @ts-expect-error — missing .compliance()
    email: fp.string()
  }
});

// ❌ plain p.string()
const Bad2 = defineComplianceEntity({
  name: 'Bad2',
  properties: {
    // @ts-expect-error — p.string() is not classified
    email: p.string()
  }
});

// ❌ fp.integer() without .compliance()
const Bad3 = defineComplianceEntity({
  name: 'Bad3',
  properties: {
    // @ts-expect-error — missing .compliance()
    count: fp.integer()
  }
});

// ❌ fp.string().nullable() without .compliance()
const Bad4 = defineComplianceEntity({
  name: 'Bad4',
  properties: {
    // @ts-expect-error — chained but missing .compliance()
    name: fp.string().nullable()
  }
});

// ❌ fp.enum() without .compliance()
const Bad5 = defineComplianceEntity({
  name: 'Bad5',
  properties: {
    // @ts-expect-error — missing .compliance() on enum
    role: fp.enum(['admin', 'user'] as const)
  }
});

// ❌ fp.json() without .compliance()
const Bad6 = defineComplianceEntity({
  name: 'Bad6',
  properties: {
    // @ts-expect-error — missing .compliance() on json
    data: fp.json()
  }
});

// ---------------------------------------------------------------------------
// ✅ InferEntity produces clean types — no __classified brand leaking
// ---------------------------------------------------------------------------

// Exact type assertions: these fail if __classified leaks into the value type
type AssertExact<T, U> = [T] extends [U]
  ? [U] extends [T]
    ? true
    : false
  : false;

type _CheckId = AssertExact<UserType['id'], string>;
const _checkId: _CheckId = true;

type _CheckEmail = AssertExact<UserType['email'], string>;
const _checkEmail: _CheckEmail = true;

type _CheckAge = AssertExact<UserType['age'], number>;
const _checkAge: _CheckAge = true;

type _CheckActive = AssertExact<UserType['active'], boolean>;
const _checkActive: _CheckActive = true;

// nullable fields may include undefined depending on MikroORM's inference
type _CheckMedical = AssertExact<
  UserType['medicalRecord'],
  string | null | undefined
>;
const _checkMedical: _CheckMedical = true;

type _CheckStatus = AssertExact<UserType['status'], 'active' | 'inactive'>;
const _checkStatus: _CheckStatus = true;

type _CheckTags = AssertExact<UserType['tags'], string[]>;
const _checkTags: _CheckTags = true;

// ✅ Entity types are assignable to plain object shapes (em.create / em.find result)
const plainUser: { id: string; email: string; age: number; active: boolean } =
  {} as UserType;

// ✅ __classified does NOT appear as a key on the inferred entity
type UserKeys = keyof UserType;
type _NoClassified = AssertExact<
  '__classified' extends UserKeys ? true : false,
  false
>;
const _noClassified: _NoClassified = true;

// ✅ Cross-entity manyToOne: __classified doesn't cascade through relations
const Order = defineComplianceEntity({
  name: 'Order',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    total: fp.integer().compliance('none'),
    user: () => fp.manyToOne(() => User)
  }
});
type OrderType = InferEntity<typeof Order>;
const _orderTotal: number = ({} as OrderType).total;

// __classified must not appear on related entity types
type OrderKeys = keyof OrderType;
type _NoClassifiedOnOrder = AssertExact<
  '__classified' extends OrderKeys ? true : false,
  false
>;
const _noClassifiedOnOrder: _NoClassifiedOnOrder = true;

// ✅ Nested reference: entity referencing another entity that itself has manyToOne
const LineItem = defineComplianceEntity({
  name: 'LineItem',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    quantity: fp.integer().compliance('none'),
    order: () => fp.manyToOne(() => Order)
  }
});
type LineItemType = InferEntity<typeof LineItem>;
const _qty: number = ({} as LineItemType).quantity;

// ✅ manyToMany: Collection<T> infers concrete type, not Collection<any>
// (mirrors blueprint Role → Permission pattern)
const Permission = defineComplianceEntity({
  name: 'Permission',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    slug: fp.string().compliance('none')
  }
});
type PermissionType = InferEntity<typeof Permission>;

const Role = defineComplianceEntity({
  name: 'Role',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    name: fp.string().compliance('none'),
    permissions: () => fp.manyToMany(Permission)
  }
});
type RoleType = InferEntity<typeof Role>;

// permissions must be Collection<PermissionType>, NOT Collection<any>
type _CheckPermissions = AssertExact<
  RoleType['permissions'],
  Collection<PermissionType>
>;
const _checkPermissions: _CheckPermissions = true;

// slug must be string, not any
const _permSlug: string = ({} as PermissionType).slug;

// name must be string, not any
const _roleName: string = ({} as RoleType).name;

// ✅ onCreate / onUpdate / default: fields with these remain optional (Opt<>)
const WithLifecycle = defineComplianceEntity({
  name: 'WithLifecycle',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    createdAt: fp
      .datetime()
      .onCreate(() => new Date())
      .compliance('none'),
    updatedAt: fp
      .datetime()
      .onCreate(() => new Date())
      .onUpdate(() => new Date())
      .compliance('none'),
    status: fp.string().default('active').compliance('none')
  }
});
type WithLifecycleType = InferEntity<typeof WithLifecycle>;

// onCreate/onUpdate/default fields should be optional in em.create() —
// they're wrapped in Opt<> which makes them assignable without providing a value.
// If __classified breaks Options inference, these lose Opt<> and become required.
const lifecycleEntity: WithLifecycleType = {
  id: 'uuid'
  // createdAt, updatedAt, status intentionally omitted — they have defaults
} as WithLifecycleType;
const _createdAt: Date = lifecycleEntity.createdAt;

void WithLifecycle;
void lifecycleEntity;
void _createdAt;
void User;
void user;
void _id;
void _email;
void _card;
void _age;
void _active;
void _status;
void Organization;
void _orgName;
void WithEnumArray;
void Bad1;
void Bad2;
void Bad3;
void Bad4;
void Bad5;
void Bad6;
void plainUser;
void _checkId;
void _checkEmail;
void _checkAge;
void _checkActive;
void _checkMedical;
void _checkStatus;
void _checkTags;
void _noClassified;
void Order;
void _orderTotal;
void _noClassifiedOnOrder;
void LineItem;
void _qty;
void Permission;
void Role;
void _checkPermissions;
void _permSlug;
void _roleName;

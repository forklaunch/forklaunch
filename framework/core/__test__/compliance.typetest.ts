/**
 * Compile-time type tests for fp + defineComplianceEntity.
 */

import type { InferEntity } from '@mikro-orm/core';
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

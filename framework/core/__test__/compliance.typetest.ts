/**
 * Compile-time type tests for fp + defineComplianceEntity.
 *
 * Tests that TypeScript correctly rejects unclassified properties
 * and accepts classified ones.
 */

import { p } from '@mikro-orm/core';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';

// ✅ Should compile — all scalar fields classified
const GoodEntity = defineComplianceEntity({
  name: 'GoodEntity',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    email: fp.string().unique().compliance('pii'),
    record: fp.string().nullable().compliance('phi'),
    cardNumber: fp.string().compliance('pci'),
    status: fp.enum(['active', 'inactive'] as const).compliance('none')
  }
});

// ✅ Should compile — relation auto-classified
const WithRelation = defineComplianceEntity({
  name: 'WithRelation',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    parent: () => fp.manyToOne(() => WithRelation).nullable()
  }
});

// ❌ fp.string() without .compliance() — error on the property line
const BadMissingCompliance = defineComplianceEntity({
  name: 'Bad1',
  properties: {
    // @ts-expect-error — missing .compliance()
    email: fp.string()
  }
});

// ❌ plain p.string() — error on the property line
const BadPlainP = defineComplianceEntity({
  name: 'Bad2',
  properties: {
    // @ts-expect-error — using p instead of fp, no .compliance()
    email: p.string()
  }
});

// Prevent unused variable warnings
void GoodEntity;
void WithRelation;
void BadMissingCompliance;
void BadPlainP;

import { describe, expect, it } from 'vitest';
import {
  type ComplianceChangeEvent,
  installComplianceChangeHooks,
  onComplianceChange
} from '../src/persistence/complianceChanges';
import { fp } from '../src/persistence/compliancePropertyBuilder';
import type { ComplianceLevel } from '../src/persistence/complianceTypes';
import { defineComplianceEntity } from '../src/persistence/defineComplianceEntity';

type Hook = (args: unknown) => void;

function fakeSchema() {
  const hooks: Record<string, Hook[]> = {};
  return {
    hooks,
    addHook(event: string, handler: Hook) {
      (hooks[event] ??= []).push(handler);
      return this;
    }
  };
}

const levels = new Map<string, ComplianceLevel>([
  ['id', 'none'],
  ['name', 'pii'],
  ['diagnosis', 'phi'],
  ['status', 'none']
]);

function capture() {
  const events: ComplianceChangeEvent[] = [];
  const remove = onComplianceChange((e) => events.push(e));
  return { events, remove };
}

describe('compliance change hooks', () => {
  it('report creates, updates and deletes of sensitive records by field name only', () => {
    const schema = fakeSchema();
    installComplianceChangeHooks(schema as never, 'Patient', levels);
    const { events, remove } = capture();
    const meta = { primaryKeys: ['id'] };
    const entity = { id: 'p1', name: 'Ada', diagnosis: 'flu', status: 'open' };
    schema.hooks.afterCreate[0]({ entity, meta });
    schema.hooks.afterUpdate[0]({
      entity,
      meta,
      changeSet: { payload: { status: 'closed', diagnosis: 'cold' } }
    });
    schema.hooks.afterDelete[0]({ entity, meta });
    remove();
    expect(events.map((e) => [e.action, e.id, e.sensitiveFields])).toEqual([
      ['create', 'p1', ['name', 'diagnosis']],
      ['update', 'p1', ['diagnosis']],
      ['delete', 'p1', ['name', 'diagnosis']]
    ]);
    expect(events[1].fields).toEqual(['status', 'diagnosis']);
    expect(JSON.stringify(events)).not.toContain('flu');
  });

  it('adds no hooks to an entity without sensitive fields', () => {
    const schema = fakeSchema();
    installComplianceChangeHooks(
      schema as never,
      'Setting',
      new Map<string, ComplianceLevel>([['id', 'none']])
    );
    expect(schema.hooks).toEqual({});
  });

  it('is installed by defineComplianceEntity on entities holding pii, phi or pci', () => {
    const schema = defineComplianceEntity({
      name: 'AuditedRecord',
      properties: {
        id: fp.uuid().primary().compliance('none'),
        email: fp.string().compliance('pii')
      }
    }) as unknown as { meta: { hooks: Record<string, unknown[]> } };
    expect(schema.meta.hooks.afterCreate?.length).toBe(1);
    expect(schema.meta.hooks.afterUpdate?.length).toBe(1);
    expect(schema.meta.hooks.afterDelete?.length).toBe(1);
  });
});

import type { EntitySchema, EventArgs } from '@mikro-orm/core';
import type { ComplianceLevel } from './complianceTypes';
import { getBoundTenantId } from './encryptedType';

/**
 * Emitted after a record of an entity holding pii, phi or pci fields is
 * created, updated or deleted. Carries field names, never values.
 */
export interface ComplianceChangeEvent {
  entity: string;
  /** Primary key, colon-joined for composite keys. */
  id: string;
  action: 'create' | 'update' | 'delete';
  /** Fields written (create: every field set; update: the changed ones). */
  fields: string[];
  /** The sensitive fields among them. */
  sensitiveFields: string[];
  tenantId: string | null;
}

export type ComplianceChangeListener = (event: ComplianceChangeEvent) => void;

const LISTENERS: Set<ComplianceChangeListener> = ((
  globalThis as Record<symbol, unknown>
)[Symbol.for('forklaunch.compliance.changeListeners')] ??=
  new Set()) as Set<ComplianceChangeListener>;

/**
 * Register a listener for every change to a sensitive record, for audit
 * trails. Returns a function that removes it.
 */
export function onComplianceChange(
  listener: ComplianceChangeListener
): () => void {
  LISTENERS.add(listener);
  return () => LISTENERS.delete(listener);
}

function emit(event: ComplianceChangeEvent) {
  for (const listener of LISTENERS) {
    try {
      listener(event);
    } catch (error) {
      // An audit sink failing must never fail the write that already happened.
      console.error('Compliance change listener failed:', error);
    }
  }
}

/**
 * Attach create/update/delete hooks to an entity schema that holds sensitive
 * fields, so every change to one of its records is reported without the app
 * wiring anything.
 */
export function installComplianceChangeHooks(
  schema: EntitySchema,
  entityName: string,
  levels: Map<string, ComplianceLevel>
): void {
  const sensitive = new Set(
    [...levels].filter(([, level]) => level !== 'none').map(([name]) => name)
  );
  if (!sensitive.size) return;
  const report =
    (action: ComplianceChangeEvent['action']) =>
    (args: EventArgs<Record<string, unknown>>) => {
      if (!LISTENERS.size) return;
      const entity = args.entity as Record<string, unknown>;
      const keys = args.meta?.primaryKeys ?? ['id'];
      const fields =
        action === 'update'
          ? Object.keys(args.changeSet?.payload ?? {})
          : action === 'create'
            ? [...levels.keys()].filter((name) => entity[name] !== undefined)
            : [];
      emit({
        entity: entityName,
        id: keys.map((key) => String(entity[key] ?? '')).join(':'),
        action,
        fields,
        sensitiveFields:
          action === 'delete'
            ? [...sensitive]
            : fields.filter((name) => sensitive.has(name)),
        tenantId: getBoundTenantId() ?? null
      });
    };
  schema.addHook('afterCreate', report('create'));
  schema.addHook('afterUpdate', report('update'));
  schema.addHook('afterDelete', report('delete'));
}

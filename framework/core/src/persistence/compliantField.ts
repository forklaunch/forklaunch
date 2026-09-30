import crypto from 'crypto';
import {
  Type,
  type Platform,
  type PrimaryKeyProp,
  type TransformContext
} from '@mikro-orm/core';
import type { EncryptedComplianceLevel } from './complianceTypes';
import {
  deserializeFromEncryption,
  getBoundTenantId,
  getRegisteredEncryptor,
  requireBoundTenantId,
  serializeForEncryption
} from './encryptedType';
import {
  EncryptionRequiredError,
  isEncryptedCiphertext,
  sealedAnon
} from './fieldEncryptor';

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/**
 * How a field's `.anon` value is derived. Shaped after the HIPAA Safe Harbor
 * identifiers: identifiers become random tokens, dates keep the year, ZIP
 * codes keep three digits (or `000` for sparsely populated areas), card
 * numbers keep the last four, everything else is redacted.
 */
export type AnonStrategy = 'token' | 'year' | 'zip3' | 'last4' | 'redact';

/** Options accepted by `.compliance(level, options)` for pii, phi and pci. */
export interface ComplianceOptions {
  /**
   * How `.anon` is derived. Defaults to `token` for strings, `year` for
   * dates and `redact` for everything else.
   */
  readonly anon?: AnonStrategy;
  /**
   * Allow equality lookups (`where: { field: value }`, `$in`, `$ne`, `$nin`).
   * Adds a `<column>_idx` blind-index column (keyed HMAC) that the query is
   * matched against; the ciphertext stays in the original column.
   */
  readonly queryable?: boolean;
  /**
   * How values are normalized before indexing. `exact` (default) matches
   * byte for byte; `lowercase` trims and lowercases strings first (emails).
   */
  readonly normalize?: 'exact' | 'lowercase';
}

/** @internal what a compliant column type knows about its property. */
export interface CompliantFieldSpec {
  level: EncryptedComplianceLevel;
  anon: AnonStrategy;
  queryable: boolean;
  normalize: 'exact' | 'lowercase';
  elementRuntimeType: string;
  isArray: boolean;
  enumValues?: readonly unknown[];
  /** `Entity.property`, filled in by defineComplianceEntity. */
  path: string;
}

// ---------------------------------------------------------------------------
// Access listeners
// ---------------------------------------------------------------------------

/** Emitted every time a compliant field's plaintext is read via `.deanon`. */
export interface ComplianceAccessEvent {
  /** `Entity.property` */
  field: string;
  level: EncryptedComplianceLevel;
  tenantId: string;
}

export type ComplianceAccessListener = (event: ComplianceAccessEvent) => void;

const LISTENERS: Set<ComplianceAccessListener> = ((
  globalThis as Record<symbol, unknown>
)[Symbol.for('forklaunch.compliance.listeners')] ??=
  new Set()) as Set<ComplianceAccessListener>;

/**
 * Register a listener for every `.deanon` read, for access audit logs.
 * Returns a function that removes it.
 */
export function onComplianceAccess(
  listener: ComplianceAccessListener
): () => void {
  LISTENERS.add(listener);
  return () => LISTENERS.delete(listener);
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/**
 * Thrown by `.anon` on a value still stored in a pre-v4 envelope under a
 * `token` strategy: there is no stored token yet, and one must never be
 * derived from the value. Run `reencryptEncryptedColumns` once to upgrade.
 */
export class LegacyCiphertextError extends Error {
  readonly name = 'LegacyCiphertextError' as const;
}

/** Thrown when a query targets a compliant field in a way it cannot answer. */
export class CompliantQueryError extends Error {
  readonly name = 'CompliantQueryError' as const;
}

// ---------------------------------------------------------------------------
// Anon
// ---------------------------------------------------------------------------

const MINT = Symbol.for('forklaunch.compliance.mint');
const ANON_BRAND = Symbol.for('forklaunch.compliance.anon');

/**
 * A de-identified value, produced only by a compliant field's `.anon`.
 * Anything that must never carry protected data (a message to a provider
 * without a BAA, a log line, an analytics event) can require this type and
 * check it at runtime with {@link isAnon}; a plain string cannot pass.
 */
export class Anon {
  readonly value!: string;

  /** @internal */
  constructor(value: string, key: symbol) {
    if (key !== MINT) {
      throw new Error('Anon values can only be produced by a compliant field');
    }
    Object.defineProperty(this, ANON_BRAND, { value: true });
    Object.defineProperty(this, 'value', { value, enumerable: true });
    Object.freeze(this);
  }
}

/** True for values produced by `.anon` (works across duplicate package copies). */
export function isAnon(value: unknown): value is Anon {
  return (
    value != null &&
    typeof value === 'object' &&
    (value as Record<symbol, unknown>)[ANON_BRAND] === true
  );
}

// Safe Harbor: three-digit ZIP prefixes covering 20,000 or fewer people must
// be replaced with 000 (HHS de-identification guidance, 2000 Census list).
const RESTRICTED_ZIP3 = new Set([
  '036',
  '059',
  '063',
  '102',
  '203',
  '556',
  '692',
  '790',
  '821',
  '823',
  '830',
  '831',
  '878',
  '879',
  '884',
  '890',
  '893'
]);

const REDACTED = '[redacted]';

/** @internal */
export function computeAnon(strategy: AnonStrategy, value: unknown): string {
  switch (strategy) {
    case 'token':
      // Random, never derived from the value: Safe Harbor forbids
      // re-identification codes computed from the individual's data.
      return 'tok_' + crypto.randomBytes(9).toString('base64url');
    case 'year': {
      if (value instanceof Date && !isNaN(value.getTime()))
        return String(value.getUTCFullYear());
      const match = /\d{4}/.exec(String(value));
      return match ? match[0] : REDACTED;
    }
    case 'zip3': {
      const digits = String(value).replace(/\D/g, '').slice(0, 3);
      if (digits.length < 3 || RESTRICTED_ZIP3.has(digits)) return '000';
      return digits;
    }
    case 'last4': {
      const digits = String(value).replace(/\D/g, '');
      return digits.length >= 4 ? `•••• ${digits.slice(-4)}` : REDACTED;
    }
    case 'redact':
      return REDACTED;
  }
}

/** The default strategy for a property's runtime type. */
export function defaultAnonStrategy(
  elementRuntimeType: string,
  isArray: boolean,
  hasEnum: boolean
): AnonStrategy {
  if (isArray || hasEnum) return 'redact';
  if (elementRuntimeType === 'string') return 'token';
  if (elementRuntimeType === 'Date') return 'year';
  return 'redact';
}

// ---------------------------------------------------------------------------
// CompliantField
// ---------------------------------------------------------------------------

interface FieldState {
  spec: CompliantFieldSpec;
  /** Plaintext is known (created in-process, or decrypted already). */
  hasPlain: boolean;
  plain?: unknown;
  /** Stored envelope, as loaded or as last sealed. */
  sealed?: string;
  /** Tenant the cached `sealed` was written under. */
  sealedTenant?: string;
  /** Blind index, as loaded or as computed. */
  index?: string | null;
  anon?: string;
}

const STATE: WeakMap<object, FieldState> = ((
  globalThis as Record<symbol, unknown>
)[Symbol.for('forklaunch.compliance.state')] ??= new WeakMap()) as WeakMap<
  object,
  FieldState
>;
const FIELD_BRAND = Symbol.for('forklaunch.compliance.field');

function stateOf(field: object): FieldState {
  const state = STATE.get(field);
  if (!state) throw new Error('Not a compliant field');
  return state;
}

/**
 * A pii, phi or pci property of an entity. The only ways to its value are
 * `.anon` (de-identified, safe to share) and `.deanon` (plaintext, reported
 * to {@link onComplianceAccess} listeners). There are deliberately no
 * `toString`, `toJSON` or inspect overloads: interpolating it prints
 * `[object Object]` and serializing it prints `{}`, so nothing leaks by
 * accident and every read is explicit.
 *
 * Plaintext is decrypted lazily, on the first `.deanon`, in the caller's
 * encryption context.
 *
 * Assigning a plain value to the property (directly, through `em.create` or
 * through `em.assign`) wraps it; `where: { property: value }` works for
 * fields declared `queryable`.
 */
export class CompliantField<
  V = unknown,
  L extends EncryptedComplianceLevel = EncryptedComplianceLevel,
  Q extends boolean = boolean
> {
  // Type-only members that teach MikroORM's EntityData / FilterQuery types
  // what may be assigned and queried. They do not exist at runtime.
  /** @internal */
  declare readonly __runtime?: CompliantField<V, L, Q> | V;
  /** @internal */
  declare readonly __raw?: string;
  /** @internal */
  declare readonly __serialized?: Record<string, never>;
  /** @internal lets `where` accept a plain value, but only for queryable fields */
  declare readonly [PrimaryKeyProp]?: Q extends true ? V : never;
  /** @internal carries the level into the type */
  declare readonly __level?: L;

  private constructor(state: FieldState) {
    Object.defineProperty(this, FIELD_BRAND, { value: true });
    STATE.set(this, state);
    Object.freeze(this);
  }

  /**
   * @internal Wrap a plaintext value for `spec`. Applications assign plain
   * values to the entity property instead.
   */
  static fromPlain<V>(value: V, spec: CompliantFieldSpec): CompliantField<V> {
    if (spec.enumValues) {
      const values = spec.isArray && Array.isArray(value) ? value : [value];
      for (const v of values) {
        if (!spec.enumValues.includes(v)) {
          throw new Error(
            `Invalid enum value${spec.path ? ` for ${spec.path}` : ''}: ${String(v)}. Allowed values: ${spec.enumValues.join(', ')}`
          );
        }
      }
    }
    return new CompliantField<V>({
      spec,
      hasPlain: true,
      plain: value,
      anon: computeAnon(spec.anon, value)
    });
  }

  /** @internal Wrap a stored value (any envelope, or legacy plaintext). */
  static fromStored<V>(
    stored: string,
    spec: CompliantFieldSpec,
    index?: string | null
  ): CompliantField<V> {
    if (!isEncryptedCiphertext(stored)) {
      // Written before encryption was switched on: the column holds the
      // serialized plaintext. It is sealed on the next write.
      return new CompliantField<V>({
        spec,
        hasPlain: true,
        plain: deserializeFromEncryption(
          stored,
          spec.elementRuntimeType,
          spec.isArray
        ),
        index
      });
    }
    return new CompliantField<V>({
      spec,
      hasPlain: false,
      sealed: stored,
      anon: sealedAnon(stored) ?? undefined,
      index
    });
  }

  /** @internal A queryable field whose ciphertext column was not selected. */
  static fromIndexOnly<V>(
    index: string | null,
    spec: CompliantFieldSpec
  ): CompliantField<V> {
    return new CompliantField<V>({ spec, hasPlain: false, index });
  }

  /** The field's classification. */
  get level(): L {
    return stateOf(this).spec.level as L;
  }

  /** The de-identified value. Never decrypts for values in a `v4` envelope. */
  get anon(): Anon {
    const state = stateOf(this);
    if (state.anon === undefined) {
      if (state.spec.anon === 'token') {
        throw new LegacyCiphertextError(
          `${state.spec.path} is stored in a pre-v4 envelope and has no token yet; run reencryptEncryptedColumns() once to upgrade it`
        );
      }
      state.anon = computeAnon(state.spec.anon, readPlaintext(this, false));
    }
    return new Anon(state.anon, MINT);
  }

  /** The plaintext. Every read is reported to access listeners. */
  get deanon(): V {
    return readPlaintext(this, true) as V;
  }
}

function readPlaintext(field: CompliantField, report: boolean): unknown {
  const state = stateOf(field);
  let tenantId = '';
  if (!state.hasPlain) {
    tenantId = requireBoundTenantId('read');
    if (state.sealed === undefined) {
      throw new Error(
        `${state.spec.path} was not loaded: its ciphertext column was left out of the selection`
      );
    }
    const encryptor = getRegisteredEncryptor();
    if (!encryptor) {
      throw new EncryptionRequiredError(
        `Cannot read ${state.spec.path}: no encryptor registered. Call registerEncryptor() at bootstrap.`
      );
    }
    const opened = encryptor.open(state.sealed, tenantId);
    state.plain = deserializeFromEncryption(
      opened.plaintext,
      state.spec.elementRuntimeType,
      state.spec.isArray
    );
    state.hasPlain = true;
  }
  if (report) {
    tenantId ||= getBoundTenantId() ?? '';
    for (const listener of LISTENERS) {
      listener({ field: state.spec.path, level: state.spec.level, tenantId });
    }
  }
  return state.plain;
}

/** True for compliant field instances (works across duplicate package copies). */
export function isCompliantField(value: unknown): value is CompliantField {
  return (
    value != null &&
    typeof value === 'object' &&
    (value as Record<symbol, unknown>)[FIELD_BRAND] === true
  );
}

// ---------------------------------------------------------------------------
// Sealing and indexing (flush time, inside the caller's encryption context)
// ---------------------------------------------------------------------------

function requireEncryptor(path: string) {
  const encryptor = getRegisteredEncryptor();
  if (!encryptor) {
    throw new EncryptionRequiredError(
      `Cannot write ${path}: no encryptor registered. Call registerEncryptor() at bootstrap; compliant fields are never stored in plaintext.`
    );
  }
  return encryptor;
}

/** @internal The stored envelope for a field, sealing it on first use. */
export function sealedValueOf(field: CompliantField): string | undefined {
  const state = stateOf(field);
  // Loaded values (sealedTenant unset) keep their stored bytes; values sealed
  // in-process are reused while the tenant is unchanged. Reusing the bytes is
  // what keeps dirty checking quiet: a fresh random IV would differ on every
  // flush.
  if (state.sealed !== undefined && state.sealedTenant === undefined) {
    return state.sealed;
  }
  if (!state.hasPlain) return state.sealed;
  const tenantId = requireBoundTenantId('write');
  if (state.sealed !== undefined && state.sealedTenant === tenantId) {
    return state.sealed;
  }
  if (state.anon === undefined) {
    state.anon = computeAnon(state.spec.anon, state.plain);
  }
  state.sealed = requireEncryptor(state.spec.path).seal(
    serializeForEncryption(state.plain),
    tenantId,
    state.anon
  );
  state.sealedTenant = tenantId;
  return state.sealed;
}

/** @internal Normalize an already-serialized value for indexing. */
export function normalizeSerializedForIndex(
  serialized: string,
  spec: CompliantFieldSpec
): string {
  return spec.normalize === 'lowercase'
    ? serialized.trim().toLowerCase()
    : serialized;
}

function normalizedForIndex(value: unknown, spec: CompliantFieldSpec): string {
  return normalizeSerializedForIndex(serializeForEncryption(value), spec);
}

/** @internal Blind index of a raw query value. */
export function blindIndexOfValue(
  value: unknown,
  spec: CompliantFieldSpec
): string {
  return requireEncryptor(spec.path).blindIndex(
    normalizedForIndex(value, spec),
    requireBoundTenantId('query')
  );
}

/** @internal The blind index for a field, computing it when missing. */
export function blindIndexOf(field: CompliantField): string | null {
  const state = stateOf(field);
  if (state.index) return state.index;
  if (!state.hasPlain && state.sealed === undefined) return state.index ?? null;
  // Rows written before the field was queryable have no index yet: compute
  // it, so the next flush backfills the column.
  try {
    const plain = state.hasPlain ? state.plain : readPlaintext(field, false);
    state.index = blindIndexOfValue(plain, state.spec);
  } catch {
    return state.index ?? null;
  }
  return state.index;
}

// ---------------------------------------------------------------------------
// MikroORM types
// ---------------------------------------------------------------------------

const PENDING = Symbol.for('forklaunch.compliance.pending');

/** @internal Placeholder hydrated into a queryable property before its ciphertext is paired with it. */
export interface PendingIndex {
  readonly [PENDING]: string | null;
}

export function isPendingIndex(value: unknown): value is PendingIndex {
  return value != null && typeof value === 'object' && PENDING in value;
}

export function pendingIndexValue(value: PendingIndex): string | null {
  return value[PENDING];
}

abstract class CompliantTypeBase extends Type<unknown, string | null> {
  constructor(readonly spec: CompliantFieldSpec) {
    super();
  }

  override getColumnType(): string {
    return 'text';
  }

  override get runtimeType(): string {
    return 'string';
  }

  override ensureComparable(): boolean {
    return false;
  }

  override compareAsType(): string {
    return 'string';
  }
}

/**
 * Column type of a non-queryable compliant field. The column holds the
 * `v4` envelope; loading yields a {@link CompliantField}.
 */
export class CompliantType extends CompliantTypeBase {
  override convertToDatabaseValue(
    value: unknown,
    _platform: Platform,
    context?: TransformContext
  ): string | null {
    if (context?.fromQuery) {
      throw new CompliantQueryError(
        `${this.spec.path} cannot be queried: declare it with .compliance('${this.spec.level}', { queryable: true }) to allow equality lookups`
      );
    }
    if (value === null || value === undefined) return null;
    if (isCompliantField(value)) return sealedValueOf(value) ?? null;
    if (isEncryptedCiphertext(value)) return value;
    return sealedValueOf(CompliantField.fromPlain(value, this.spec)) ?? null;
  }

  override convertToJSValue(value: string | null): unknown {
    if (value === null || value === undefined) return value;
    if (isCompliantField(value)) return value;
    return CompliantField.fromStored(String(value), this.spec);
  }
}

const EQUALITY_OPERATORS = new Set(['$eq', '$ne', '$in', '$nin']);

/**
 * Column type of a queryable compliant field. The property's own column
 * (`<column>_idx`) holds the blind index, so `where` compiles to an
 * index comparison; the envelope lives in a hidden sibling column.
 */
export class CompliantIndexType extends CompliantTypeBase {
  override convertToDatabaseValue(
    value: unknown,
    _platform: Platform,
    context?: TransformContext
  ): string | null {
    if (value === null || value === undefined) return null;
    if (context?.fromQuery) {
      const key = context.key;
      if (
        key !== undefined &&
        !EQUALITY_OPERATORS.has(String(key)) &&
        !/^\d+$/.test(String(key))
      ) {
        throw new CompliantQueryError(
          `${this.spec.path} supports equality lookups only ($eq, $ne, $in, $nin); ${String(key)} would need the plaintext`
        );
      }
    }
    if (isCompliantField(value)) return blindIndexOf(value);
    if (isPendingIndex(value)) return pendingIndexValue(value);
    if (typeof value === 'string' && value.startsWith('bi1:')) return value;
    return blindIndexOfValue(value, this.spec);
  }

  override convertToJSValue(value: string | null): unknown {
    if (isCompliantField(value) || isPendingIndex(value)) return value;
    return { [PENDING]: value ?? null } satisfies PendingIndex;
  }
}

/** True for the column types above. */
export function isCompliantType(
  type: unknown
): type is CompliantType | CompliantIndexType {
  return type instanceof CompliantType || type instanceof CompliantIndexType;
}

// ---------------------------------------------------------------------------
// Entity accessors
// ---------------------------------------------------------------------------

/** @internal One compliant property of an entity class. */
export interface CompliantPropertyBinding {
  property: string;
  spec: CompliantFieldSpec;
  /** Hidden property holding the envelope, for queryable fields. */
  sibling?: string;
}

type Slots = Record<string, unknown>;
const SLOTS: WeakMap<object, Slots> = ((globalThis as Record<symbol, unknown>)[
  Symbol.for('forklaunch.compliance.slots')
] ??= new WeakMap()) as WeakMap<object, Slots>;

function slotsOf(entity: object): Slots {
  let slots = SLOTS.get(entity);
  if (!slots) {
    slots = {};
    SLOTS.set(entity, slots);
  }
  return slots;
}

function readProperty(
  entity: object,
  binding: CompliantPropertyBinding
): CompliantField | null | undefined {
  const slots = slotsOf(entity);
  const value = slots[binding.property];
  if (binding.sibling) {
    const stored = slots[binding.sibling];
    if (isPendingIndex(value)) {
      const index = pendingIndexValue(value);
      const field =
        typeof stored === 'string'
          ? CompliantField.fromStored(stored, binding.spec, index)
          : CompliantField.fromIndexOnly(index, binding.spec);
      slots[binding.property] = field;
      return field;
    }
    // A row written before the field was queryable has no index yet (the
    // index column hydrates as null); the envelope alone is enough to load
    // it, and the next flush backfills the index.
    if (value == null && typeof stored === 'string') {
      const field = CompliantField.fromStored(stored, binding.spec, null);
      slots[binding.property] = field;
      return field;
    }
  }
  return value as CompliantField | null | undefined;
}

function sameRules(a: CompliantFieldSpec, b: CompliantFieldSpec): boolean {
  return (
    a === b ||
    (a.path === b.path &&
      a.anon === b.anon &&
      a.normalize === b.normalize &&
      a.queryable === b.queryable)
  );
}

function writeProperty(
  entity: object,
  binding: CompliantPropertyBinding,
  value: unknown
): void {
  const slots = slotsOf(entity);
  if (isPendingIndex(value)) {
    slots[binding.property] = value;
    return;
  }
  if (value === null || value === undefined) {
    slots[binding.property] = value;
    // Clearing the field clears its envelope too. Hydration writes the index
    // column before the envelope column (the sibling is declared right after
    // the property), so a null index hydrated for a legacy row does not wipe
    // the envelope that follows it.
    if (binding.sibling) slots[binding.sibling] = value;
    return;
  }
  if (isCompliantField(value)) {
    // A field copied from another property takes this property's rules
    // (anon strategy, index normalization): re-wrap its plaintext.
    slots[binding.property] = sameRules(stateOf(value).spec, binding.spec)
      ? value
      : CompliantField.fromPlain(readPlaintext(value, false), binding.spec);
    return;
  }
  slots[binding.property] = CompliantField.fromPlain(value, binding.spec);
}

/**
 * Install getter/setter pairs for compliant properties on an entity
 * prototype. Assigning a plain value wraps it in a {@link CompliantField};
 * reading returns the field.
 */
export function installCompliantAccessors(
  prototype: object,
  bindings: readonly CompliantPropertyBinding[]
): void {
  installInspect(prototype, bindings);
  for (const binding of bindings) {
    Object.defineProperty(prototype, binding.property, {
      configurable: true,
      enumerable: true,
      get(this: object) {
        return readProperty(this, binding);
      },
      set(this: object, value: unknown) {
        writeProperty(this, binding, value);
      }
    });
    if (binding.sibling) {
      const sibling = binding.sibling;
      Object.defineProperty(prototype, sibling, {
        configurable: true,
        enumerable: false,
        get(this: object) {
          const field = readProperty(this, binding);
          const stored = slotsOf(this)[sibling];
          if (field === null || field === undefined) return stored ?? field;
          return sealedValueOf(field) ?? stored;
        },
        set(this: object, value: unknown) {
          slotsOf(this)[sibling] = value;
        }
      });
    }
  }
}

const INSPECT = Symbol.for('nodejs.util.inspect.custom');

/**
 * MikroORM installs a custom inspect on entity prototypes only when none is
 * present (`??=`). Installing ours first keeps the hidden envelope columns
 * of queryable fields out of `console.log(entity)`, and shows compliant
 * properties (which live on the prototype) as `CompliantField {}`.
 */
function installInspect(
  prototype: object,
  bindings: readonly CompliantPropertyBinding[]
): void {
  const hidden = new Set(
    bindings.map((b) => b.sibling).filter((s): s is string => !!s)
  );
  Object.defineProperty(prototype, INSPECT, {
    configurable: true,
    writable: true,
    value(
      this: Record<string, unknown>,
      depth: number,
      options: Record<string, unknown>,
      inspect: (value: unknown, options: Record<string, unknown>) => string
    ) {
      const shown: Record<string, unknown> = {};
      for (const key of Object.keys(this)) {
        if (!hidden.has(key)) shown[key] = this[key];
      }
      for (const binding of bindings) {
        const value = this[binding.property];
        if (value !== undefined) shown[binding.property] = value;
      }
      const name = (this.constructor as { name?: string })?.name ?? 'Entity';
      return `${name} ${inspect(shown, { ...options, depth })}`;
    }
  });
}

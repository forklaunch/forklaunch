// Shared HTTP/SSE/WebSocket schema validation. No transport or Node dependencies.
import { Ajv, type AnySchema, type ValidateFunction } from 'ajv';
import { Ajv2020 } from 'ajv/dist/2020.js';
import { Ajv2019 } from 'ajv/dist/2019.js';
import AjvDraft4Module from 'ajv-draft-04';
import addFormatsModule, { type FormatsPlugin } from 'ajv-formats';

function socketDecimal(value: number): [bigint, number] {
  const [coefficient, exponent = '0'] = String(value).toLowerCase().split('e');
  const [integer, fraction = ''] = coefficient.split('.');
  return [BigInt(integer + fraction), Number(exponent) - fraction.length];
}
function socketMultipleOf(value: number, divisor: number): boolean {
  let [numerator, scale] = socketDecimal(value);
  let [denominator, divisorScale] = socketDecimal(divisor);
  scale -= divisorScale;
  if (scale >= 0) numerator *= 10n ** BigInt(scale);
  else denominator *= 10n ** BigInt(-scale);
  return numerator % denominator === 0n;
}
function socketValidator(schema: AnySchema, regexExecution?: ZodRegexExecution): ValidateFunction {
  const dialect = typeof schema === 'object' ? schema.$schema : undefined;
  // CommonJS constructor declarations are exposed as a namespace in NodeNext.
  const AjvDraft4 = AjvDraft4Module as unknown as typeof Ajv;
  const engine =
    typeof dialect === 'string' && dialect.includes('/draft-04/')
      ? new AjvDraft4({
          strict: false,
          validateFormats: true,
          ownProperties: true,
        })
      : typeof dialect === 'string' && dialect.includes('2020-12')
        ? new Ajv2020({
            strict: false,
            validateFormats: true,
            ownProperties: true,
          })
        : typeof dialect === 'string' && dialect.includes('2019-09')
          ? new Ajv2019({
              strict: false,
              validateFormats: true,
              ownProperties: true,
            })
          : new Ajv({
              strict: false,
              validateFormats: true,
              ownProperties: true,
            });
  if (typeof dialect === 'string' && dialect.includes('/draft-06/'))
    engine.addMetaSchema(socketDraft6MetaSchema);
  if (typeof dialect === 'string' && dialect.includes('/draft-04/')) {
    // The Draft 4 adapter also registers newer AJV applicator/const keywords.
    // They are annotations in this dialect and must not narrow accepted values.
    for (const keyword of [
      'const',
      'contains',
      'propertyNames',
      'if',
      'then',
      'else',
    ])
      engine.removeKeyword(keyword);
  }
  if (typeof dialect === 'string' && dialect.includes('/draft-06/'))
    for (const keyword of ['if', 'then', 'else']) engine.removeKeyword(keyword);
  // CJS package declarations are exposed as a namespace under NodeNext ESM.
  const addFormats = addFormatsModule as unknown as FormatsPlugin;
  addFormats(engine);
  registerZodChecks(engine);
  registerZodRegex(engine, regexExecution);
  engine.addFormat('forklaunch-whatwg-url', {
    type: 'string',
    validate: (value: string) => {
      try { new URL(value); return true; } catch { return false; }
    },
  });
  // Native (Rust/schemars) integer formats: annotation-only bounds checks, so
  // contracts stay unchanged and Ajv does not warn `unknown format "uint64"`.
  for (const [name, min, max] of [['int8', -128, 127], ['int16', -32768, 32767], ['uint8', 0, 255], ['uint16', 0, 65535], ['uint32', 0, 4294967295], ['uint64', 0, Infinity], ['uint', 0, Infinity], ['int', -Infinity, Infinity], ['int128', -Infinity, Infinity], ['uint128', 0, Infinity]] as const)
    engine.addFormat(name, { type: 'number', validate: (value: number) => Number.isInteger(value) && value >= min && value <= max });
  engine.removeKeyword('multipleOf');
  engine.addKeyword({
    keyword: 'multipleOf',
    type: 'number',
    schemaType: 'number',
    compile: (divisor: number) => (value: number) =>
      socketMultipleOf(value, divisor),
  });
  // No loadSchema callback: external resources are never fetched.
  return engine.compile(socketAjvSchema(schema));
}
// AJV excludes properties named __proto__ from ordinary property compilation.
// Keep their original pointer targets and also validate them as exact patterns.
function socketAjvSchema(schema: AnySchema): AnySchema {
  const copy = JSON.parse(JSON.stringify(schema)) as AnySchema;
  const dictionaries = new Set([
    'properties',
    'patternProperties',
    '$defs',
    'definitions',
    'dependentSchemas',
    'dependencies',
  ]);
  const singles = new Set([
    'additionalProperties',
    'additionalItems',
    'unevaluatedProperties',
    'unevaluatedItems',
    'contains',
    'not',
    'if',
    'then',
    'else',
    'propertyNames',
    'contentSchema',
  ]);
  const dialect = typeof copy === 'object' ? copy.$schema : undefined;
  const draft4 = typeof dialect === 'string' && dialect.includes('/draft-04/');
  const draft6 = typeof dialect === 'string' && dialect.includes('/draft-06/');
  const draft7 =
    dialect === undefined ||
    (typeof dialect === 'string' && dialect.includes('/draft-07/'));
  const draft2019 =
    typeof dialect === 'string' && dialect.includes('/draft/2019-09/');
  const draft2020 =
    typeof dialect === 'string' && dialect.includes('/draft/2020-12/');
  const legacy = draft4 || draft6 || draft7;
  const hasResourceId = (object: Record<string, unknown>, resource: unknown): boolean => {
    const id = object[draft4 ? 'id' : '$id'];
    // Legacy fragment IDs name anchors within the enclosing resource. They do
    // not change the document against which a JSON Pointer fragment resolves.
    if (typeof id !== 'string' || id === '' || id.startsWith('#')) return false;
    const parentId = resource && typeof resource === 'object'
      ? (resource as Record<string, unknown>)[draft4 ? 'id' : '$id'] : undefined;
    // The absolute spelling of an anchor in this same document has identical
    // scope. A distinct document URI still starts a nested resource.
    return !(id.includes('#') && typeof parentId === 'string' &&
      id.split('#', 1)[0] === parentId.split('#', 1)[0]);
  };
  const ignored = new Set([
    // Model identity is an annotation. AJV's resource walk also visits unknown
    // keywords and otherwise mistakes its repeated `id` for a Draft 4 resource.
    'x-forklaunch-model',
    ...(draft4 ? ['const', 'contains', 'propertyNames'] : []),
    ...(draft4 || draft6 ? ['if', 'then', 'else'] : []),
    ...(legacy
      ? [
          'dependentSchemas',
          'dependentRequired',
          'unevaluatedProperties',
          'unevaluatedItems',
          'contentSchema',
          'minContains',
          'maxContains',
        ]
      : []),
    ...(legacy || draft2019 ? ['prefixItems'] : []),
    ...(draft2020 ? ['additionalItems'] : []),
  ]);
  // A pointer can make literal data act as a schema too. Repairing that data
  // would change const/enum equality or defaults, so reject the dual-use case.
  const literalRanges: string[][] = [];
  const repairPaths: string[][] = [];
  const schemaLocations = new Map<object, { path: string[]; resourcePath: string[]; resource: unknown }>();
  const checked = new Set<string>();
  const inspect: Array<{ node: unknown; path: string[]; resource: unknown; resourcePath: string[] }> = [
    { node: copy, path: [], resource: copy, resourcePath: [] },
  ];
  let remaining = 10000;
  while (inspect.length) {
    const { node, path, resource, resourcePath } = inspect.pop()!;
    const location = JSON.stringify(path);
    if (checked.has(location)) continue;
    checked.add(location);
    if (--remaining < 0 || path.length > 64)
      throw new Error('Socket schema inspection budget exceeded');
    if (!node || typeof node !== 'object' || Array.isArray(node)) continue;
    const object = node as Record<string, unknown>;
    const ownsResource = hasResourceId(object, resource);
    const base = ownsResource ? node : resource;
    const basePath = ownsResource ? path : resourcePath;
    schemaLocations.set(node, { path, resourcePath: basePath, resource: base });
    for (const key of ['default', 'const', 'enum']) {
      if (Object.prototype.hasOwnProperty.call(object, key)) literalRanges.push([...path, key]);
    }
    if (object.properties && typeof object.properties === 'object' &&
        Object.prototype.hasOwnProperty.call(object.properties, '__proto__')) repairPaths.push(path);
    const push = (child: unknown, childPath: string[]) => {
      if (inspect.length >= remaining) throw new Error('Socket schema inspection budget exceeded');
      inspect.push({ node: child, path: childPath, resource: base, resourcePath: basePath });
    };
    if (typeof object.$ref === 'string' && object.$ref.startsWith('#')) {
      const pointer = decodeURIComponent(object.$ref.slice(1));
      if (pointer.startsWith('/')) {
        const tokens = pointer.slice(1).split('/').map(key => key.replace(/~1/g, '/').replace(/~0/g, '~'));
        let target: unknown = base;
        for (const key of tokens) target = target && typeof target === 'object' &&
          Object.prototype.hasOwnProperty.call(target, key) ? (target as Record<string, unknown>)[key] : undefined;
        push(target, [...basePath, ...tokens]);
      }
    }
    for (const [key, child] of Object.entries(object)) {
      if (ignored.has(key)) continue;
      if (dictionaries.has(key) && child && typeof child === 'object' && !Array.isArray(child)) {
        for (const [name, value] of Object.entries(child)) push(value, [...path, key, name]);
      } else if (singles.has(key)) push(child, [...path, key]);
      else if (['items', 'prefixItems', 'allOf', 'anyOf', 'oneOf'].includes(key)) {
        if (Array.isArray(child)) child.forEach((value, index) => push(value, [...path, key, String(index)]));
        else push(child, [...path, key]);
      }
    }
  }
  function assertNotLiteral(path: string[], operation: string): void {
    for (const range of literalRanges) {
      if (--remaining < 0) throw new Error('Socket schema inspection budget exceeded');
      if (range.length <= path.length && range.every((key, index) => path[index] === key))
        throw new Error(`Socket ${operation} cannot rewrite literal default/const/enum data`);
    }
  }
  for (const path of repairPaths) assertNotLiteral(path, 'own-property repair');
  let activeSchemas: WeakSet<object> | undefined;
  if (ignored.size) {
    const active = new WeakSet<object>();
    activeSchemas = active;
    const pending: Array<{ node: unknown; resource: unknown }> = [{ node: copy, resource: copy }];
    const annotations: Array<{ value: unknown; path: string[] }> = [];
    // AJV discovers resource IDs before consulting its dialect vocabulary.
    // Identify real schema positions, including explicit local references into
    // ignored annotation storage, before cleaning its private compilation copy.
    while (pending.length) {
      const { node, resource } = pending.pop()!;
      if (
        !node ||
        typeof node !== 'object' ||
        Array.isArray(node) ||
        active.has(node)
      )
        continue;
      active.add(node);
      const object = node as Record<string, unknown>;
      const base = schemaLocations.get(node)?.resource ?? resource;
      const push = (child: unknown) => pending.push({ node: child, resource: base });
      if (typeof object.$ref === 'string' && object.$ref.startsWith('#')) {
        const pointer = decodeURIComponent(object.$ref.slice(1));
        let target: unknown = base;
        if (pointer.startsWith('/')) {
          for (const segment of pointer.slice(1).split('/')) {
            const key = segment.replace(/~1/g, '/').replace(/~0/g, '~');
            target =
              target &&
              typeof target === 'object' &&
              Object.prototype.hasOwnProperty.call(target, key)
                ? (target as Record<string, unknown>)[key]
                : undefined;
          }
        } else if (pointer !== '') target = undefined;
        push(target);
      }
      for (const [key, child] of Object.entries(object)) {
        if (ignored.has(key)) {
          const location = schemaLocations.get(node);
          if (!location) throw new Error('Socket schema location unavailable');
          annotations.push({ value: child, path: [...location.path, key] });
        }
        else if (
          dictionaries.has(key) &&
          child &&
          typeof child === 'object' &&
          !Array.isArray(child)
        ) {
          for (const value of Object.values(child)) push(value);
        } else if (singles.has(key)) push(child);
        else if (
          ['items', 'prefixItems', 'allOf', 'anyOf', 'oneOf'].includes(key)
        ) {
          if (Array.isArray(child)) {
            for (const value of child) push(value);
          } else push(child);
        }
      }
    }
    function cleanAnnotation(value: unknown, path: string[]): void {
      if (!value || typeof value !== 'object' || active.has(value)) return;
      if (Array.isArray(value)) {
        value.forEach((child, index) => cleanAnnotation(child, [...path, String(index)]));
        return;
      }
      const object = value as Record<string, unknown>;
      for (const keyword of ['$id', 'id', '$anchor', '$dynamicAnchor']) {
        if (typeof object[keyword] !== 'string') continue;
        assertNotLiteral(path, 'annotation cleanup');
        delete object[keyword];
      }
      for (const [key, child] of Object.entries(object)) cleanAnnotation(child, [...path, key]);
    }
    for (const annotation of annotations) cleanAnnotation(annotation.value, annotation.path);
  }
  const visited = new WeakSet<object>();
  function visit(node: unknown, location: string[]): void {
    if (!node || typeof node !== 'object' || Array.isArray(node)) return;
    if (activeSchemas && !activeSchemas.has(node)) return;
    if (visited.has(node)) return;
    visited.add(node);
    const object = node as Record<string, unknown>;
    if (typeof object.$schema === 'string') {
      let normalized = object.$schema;
      if (object.$schema.includes('/draft-04/'))
        normalized = 'http://json-schema.org/draft-04/schema#';
      else if (object.$schema.includes('/draft-06/'))
        normalized = 'http://json-schema.org/draft-06/schema#';
      else if (object.$schema.includes('/draft-07/'))
        normalized = 'http://json-schema.org/draft-07/schema#';
      else if (object.$schema.includes('/draft/2019-09/'))
        normalized = 'https://json-schema.org/draft/2019-09/schema';
      else if (object.$schema.includes('/draft/2020-12/'))
        normalized = 'https://json-schema.org/draft/2020-12/schema';
      if (normalized !== object.$schema) {
        const path = schemaLocations.get(node)?.path;
        if (!path) throw new Error('Socket schema location unavailable');
        assertNotLiteral(path, 'dialect normalization');
        object.$schema = normalized;
      }
    }
    const base = schemaLocations.get(node)?.resource === node ? [] : location;
    const properties = object.properties as Record<string, unknown> | undefined;
    if (
      properties &&
      Object.prototype.hasOwnProperty.call(properties, '__proto__')
    ) {
      const patterns = (object.patternProperties ??=
        Object.create(null)) as Record<string, unknown>;
      let pattern = '^__proto__(?![\\s\\S])';
      // Existing patterns may be $ref targets; retain their original meaning.
      while (Object.prototype.hasOwnProperty.call(patterns, pattern))
        pattern += '(?:)';
      const pointer = [...base, 'properties', '__proto__']
        .map((token) =>
          encodeURIComponent(token.replace(/~/g, '~0').replace(/\//g, '~1')),
        )
        .join('/');
      const reference = { $ref: `#/${pointer}` };
      patterns[pattern] = reference;
    }
    for (const [key, child] of Object.entries(object)) {
      if (
        dictionaries.has(key) &&
        child &&
        typeof child === 'object' &&
        !Array.isArray(child)
      ) {
        for (const [name, value] of Object.entries(child))
          visit(value, [...base, key, name]);
      } else if (singles.has(key)) visit(child, [...base, key]);
      else if (
        ['items', 'prefixItems', 'allOf', 'anyOf', 'oneOf'].includes(key)
      ) {
        if (Array.isArray(child))
          child.forEach((value, index) =>
            visit(value, [...base, key, String(index)]),
          );
        else visit(child, [...base, key]);
      }
    }
  }
  visit(copy, []);
  // Explicit local pointers can select schemas under unknown annotation keys.
  // Those targets need the same own-property repair as ordinary schema fields.
  for (const [node, location] of schemaLocations)
    visit(node, location.path.slice(location.resourcePath.length));
  return copy;
}

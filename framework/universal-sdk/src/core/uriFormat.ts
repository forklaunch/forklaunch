import type Ajv from 'ajv';

// A reserved factory format, distinct from arbitrary RFC 3986 uri schemas.
export function registerFactoryUriFormat(ajv: Ajv): Ajv {
  ajv.addFormat('forklaunch-whatwg-url', {
    type: 'string',
    validate: (value: string) => {
      try {
        new URL(value);
        return true;
      } catch {
        return false;
      }
    }
  });
  const engine = ajv;
  // Native (Rust/schemars) integer formats: annotation-only bounds checks, so
  // contracts stay unchanged and Ajv does not warn `unknown format "uint64"`.
  for (const [name, min, max] of [['int8', -128, 127], ['int16', -32768, 32767], ['uint8', 0, 255], ['uint16', 0, 65535], ['uint32', 0, 4294967295], ['uint64', 0, Infinity], ['uint', 0, Infinity], ['int', -Infinity, Infinity], ['int128', -Infinity, Infinity], ['uint128', 0, Infinity]] as const)
    engine.addFormat(name, { type: 'number', validate: (value: number) => Number.isInteger(value) && value >= min && value <= max });
  return ajv;
}

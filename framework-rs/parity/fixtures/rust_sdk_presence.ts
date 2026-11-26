import { createForkLaunchSdk, type PresencePresenceWritePresenceRequest } from './index.js';

export async function verify(baseUrl: string, cases: { value: unknown; valid: boolean; normalized?: unknown }[]) {
  const sdk = createForkLaunchSdk({ baseUrl });
  const minimal: PresencePresenceWritePresenceRequest = { body: { required: null, list: [] } };
  const full: PresencePresenceWritePresenceRequest = { body: { required: 'value', optional: 'present', optionalNull: null, nested: { required: null, optionalNull: null }, list: [null], choice: null } };
  const escaped: PresencePresenceWritePresenceRequest = { body: { required: null, list: [], escaped: "line\n'\\\0\u2028\u2029", ["wire\t'\\\n\0"]: 'hello 水' } };
  // @ts-expect-error Required nullable fields must be present.
  const missing: PresencePresenceWritePresenceRequest = { body: { list: [] } };
  // @ts-expect-error Optional nonnullable fields reject null.
  const invalid: PresencePresenceWritePresenceRequest = { body: { required: null, list: [], optional: null } };
  void [minimal, full, escaped, missing, invalid];
  const observed: { index: number; value: unknown; echo: unknown }[] = [];
  for (let index = 0; index < cases.length; index++) {
    const item = cases[index]!;
    if (!item.valid) continue;
    const read = await sdk.presence.presence.readPresence({ params: { index: String(index) } });
    if (read.code !== 200) throw Error(`TypeScript presence case ${index}`);
    const echo = await sdk.presence.presence.writePresence({ body: read.response });
    if (echo.code !== 200) throw Error(`TypeScript presence echo ${index}`);
    observed.push({ index, value: read.response, echo: echo.response });
  }
  return observed;
}

import type { ItemsSdkClient, Item, TimedItem } from './sdk';
import { universalSdk } from '@forklaunch/universal-sdk';
export const client = universalSdk<ItemsSdkClient>;
export const valid: Item = {name:'valid'};
export const date: TimedItem = {at:new Date('2024-02-29T12:34:56.123Z')};
// @ts-expect-error Native timestamp DTO exposes a Date, not a wire string.
export const invalid: TimedItem = {at:'2024-02-29T12:34:56.123Z'};

export async function checkPublicMethods(sdk: ItemsSdkClient) {
  const result = await sdk.items.fetch();
  if (result.code === 200) { const name: string = result.response.name; void name; }
  const timed = await sdk.items.time({body:{at:new Date()}});
  if (timed.code === 200) { const at: Date = timed.response.at; void at; }
  // @ts-expect-error Date input cannot be a transport string.
  await sdk.items.time({body:{at:'2024-02-29T12:34:56.123Z'}});
  // @ts-expect-error Required body cannot be omitted.
  await sdk.items.time();
  const headed = await sdk.items.headed();
  if (headed.code === 200) { const revision: number = headed.headers['x-revision']; void revision; }
  const stream = await sdk.items.watch();
  if (stream.code === 200) {
    for await (const event of stream.response) { const name: string = event.data.name; void name; }
  }
}

export async function checkStatusAndMedia(sdk: ItemsSdkClient) {
  const result = await sdk.items.represent();
  if (result.status === 200) {
    if (result.contentType === 'application/json') {
      const name: string = result.response.name;
      // @ts-expect-error JSON response cannot be a text body.
      const invalid: string = result.response;
      void name; void invalid;
    } else {
      const text: string = result.response;
      // @ts-expect-error Text response has no DTO property.
      result.response.name;
      void text;
    }
  } else if (result.status === 201) {
    const name: string = result.response.name;
    const media: 'application/json' = result.contentType;
    void name; void media;
  }
}

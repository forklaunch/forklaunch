import type { Details, ObjectPayload, ObjectsSdkClient } from './sdk.js';

type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends
  (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;
type PayloadKeys = Assert<Equal<keyof ObjectPayload, 'title' | 'details' | 'labels'>>;
type NestedKeys = Assert<Equal<keyof Details, 'count'>>;

const payload: ObjectPayload = {
  title: 'Notes', details: { count: 3 }, labels: { category: 'personal' }
};
payload.title.toUpperCase();
payload.labels.arbitraryKey = 'valid dictionary value';
// @ts-expect-error Misspelled DTO keys are not an unknown-valued API.
payload.titel;
// @ts-expect-error Nested DTOs have finite declared keys too.
payload.details.cout;
// @ts-expect-error Fresh DTO literals reject undeclared keys.
const misspelled: ObjectPayload = { ...payload, titel: 'typo' };
// @ts-expect-error Known fields retain their actual types.
payload.title = 123;
// @ts-expect-error Explicit dictionary values retain their declared type.
payload.labels.arbitraryKey = 123;

async function consumer(client: ObjectsSdkClient) {
  const result = await client.payload.fetch();
  if (result.code === 200) {
    const body: ObjectPayload = result.response;
    body.title.toUpperCase();
    // @ts-expect-error SDK response narrowing retains finite keys.
    body.titel;
  }
}
void consumer;

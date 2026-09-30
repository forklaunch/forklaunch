import type { EntityManager, InferEntity } from '@mikro-orm/core';
import { defineComplianceEntity, fp } from '../../../src/persistence';

export const SmsRecord = defineComplianceEntity({
  name: 'SmsRecord',
  properties: {
    id: fp.uuid().primary().compliance('none'),
    to: fp.string().compliance('pii'),
    body: fp.string().compliance('pii'),
    note: fp.string().nullable().compliance('phi'),
    status: fp.string().compliance('none')
  }
});
type SmsRecord = InferEntity<typeof SmsRecord>;

declare function send(to: string, body: string): void;

export function toDto(r: SmsRecord): { to: string; body: string; status: string } {
  return { to: r.to, body: r.body, status: r.status };
}

export function deliver(r: SmsRecord) {
  send(r.to, `Message: ${r.body}`);
}

export function normalized(r: SmsRecord): string {
  return r.to.toLowerCase();
}

export function same(r: SmsRecord, other: string) {
  return r.to === other;
}

export function noteLength(r: SmsRecord): number | undefined {
  const note: string | undefined = r.note;
  return note?.length;
}

export function anonymised(r: SmsRecord) {
  return r.to.anon;
}

export function create(em: EntityManager) {
  return em.create(SmsRecord, {
    id: 'x',
    to: '+15550100',
    body: 'hi',
    note: null,
    status: 'queued'
  });
}

export async function lookup(em: EntityManager, to: string) {
  return em.find(SmsRecord, { to });
}

import { EntityManager } from '@mikro-orm/core';

export const HISTORY_RETENTION_DAYS = 90;

/**
 * Removes the query text from search history older than 90 days, keeping
 * the entry (class, channel, time) for usage counts.
 *
 * Done in SQL rather than through RetentionService: in @forklaunch/core
 * 2.1.0 the services bundle keeps its own copy of the retention registry,
 * so policies declared on entities are not visible to RetentionService and
 * it anonymizes nothing. Nulling a column also needs no decryption, so this
 * runs without an organization context.
 */
export async function anonymizeExpiredHistory(em: EntityManager, now: Date = new Date()): Promise<number> {
  const cutoff = new Date(now.getTime() - HISTORY_RETENTION_DAYS * 24 * 60 * 60 * 1000);
  const rows: { id: string }[] = await em.getConnection().execute(
    `update search_history
        set query = null, retention_anonymized_at = now(), updated_at = now()
      where retention_anonymized_at is null and created_at < ?
      returning id`,
    [cutoff]
  );
  return rows.length;
}

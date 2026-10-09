import { EntityManager } from '@mikro-orm/core';
import { NO_ORGANIZATION_TENANT } from './tenantEm';

// Saved searches and history are encrypted per organization, so erasure,
// export and retention run once per organization that holds such rows.

export async function organizationsOfUser(em: EntityManager, userId: string): Promise<string[]> {
  const rows: { organization_id: string }[] = await em.getConnection().execute(
    `select organization_id from saved_search where user_id = ?
     union select organization_id from search_history where user_id = ?
     union select coalesce(organization_id, ?) from generated_answer where user_id = ?`,
    [userId, userId, NO_ORGANIZATION_TENANT, userId]
  );
  return rows.map((row) => row.organization_id);
}

export async function organizationsWithUserData(em: EntityManager): Promise<string[]> {
  const rows: { organization_id: string }[] = await em.getConnection().execute(
    `select organization_id from saved_search union select organization_id from search_history
     union select coalesce(organization_id, ?) from generated_answer where query is not null`,
    [NO_ORGANIZATION_TENANT]
  );
  return rows.map((row) => row.organization_id);
}

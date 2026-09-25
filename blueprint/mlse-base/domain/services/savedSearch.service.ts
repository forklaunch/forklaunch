import { EntityManager } from '@mikro-orm/core';
import { SavedSearch } from '../../persistence/entities/savedSearch.entity';
import { SearchHistory } from '../../persistence/entities/searchHistory.entity';
import { organizationsOfUser } from '../tenants';
import { tenantEm } from '../tenantEm';

export class SavedSearchNotFoundError extends Error {
  constructor(id: string) {
    super(`Saved search '${id}' not found`);
    this.name = 'SavedSearchNotFoundError';
  }
}

const MAX_SAVED_PER_USER = 200;
const HISTORY_PAGE = 50;

/**
 * A doctor's saved searches and recent history. Every read and write is
 * bound to the organization and the user, so one organization can never see
 * another's entries, and queries are decrypted with that organization's key.
 */
export class SavedSearchService {
  constructor(private readonly em: EntityManager) {}

  async list(organizationId: string, userId: string) {
    const em = tenantEm(this.em, organizationId);
    const saved = await em.find(SavedSearch, { organizationId, userId }, { orderBy: { createdAt: 'desc' } });
    return saved.map(view);
  }

  async save(organizationId: string, userId: string, name: string, query: string, topicSlug?: string) {
    const em = tenantEm(this.em, organizationId);
    const count = await em.count(SavedSearch, { organizationId, userId });
    if (count >= MAX_SAVED_PER_USER) {
      throw new Error(`A user can keep at most ${MAX_SAVED_PER_USER} saved searches`);
    }
    const saved = em.create(SavedSearch, { organizationId, userId, name, query, topicSlug: topicSlug ?? null });
    await em.flush();
    return view(saved);
  }

  async remove(organizationId: string, userId: string, id: string) {
    const em = tenantEm(this.em, organizationId);
    const saved = await em.findOne(SavedSearch, { id, organizationId, userId });
    if (!saved) {
      throw new SavedSearchNotFoundError(id);
    }
    await em.remove(saved).flush();
  }

  async history(organizationId: string, userId: string) {
    const em = tenantEm(this.em, organizationId);
    const entries = await em.find(
      SearchHistory,
      { organizationId, userId },
      { orderBy: { createdAt: 'desc' }, limit: HISTORY_PAGE }
    );
    return entries.map((entry) => ({
      id: entry.id,
      // null for queries not stored (patient-specific, prescription,
      // emergency) and once the retention job has anonymized the entry
      ...(entry.query ? { query: entry.query } : {}),
      queryClass: entry.queryClass,
      channel: entry.channel,
      ...(entry.answerId ? { answerId: entry.answerId } : {}),
      createdAt: new Date(entry.createdAt).toISOString()
    }));
  }

  // GDPR export and erasure for the user's data in MLSE. Implemented here
  // because ComplianceDataService in @forklaunch/core 2.1.0 cannot see the
  // entities' compliance registrations (its bundle has its own registry
  // copy), so it finds nothing to export or erase.
  async exportUser(userId: string) {
    const savedSearches: ReturnType<typeof view>[] = [];
    const history: Awaited<ReturnType<SavedSearchService['history']>> = [];
    for (const organizationId of await organizationsOfUser(this.em, userId)) {
      savedSearches.push(...(await this.list(organizationId, userId)));
      const em = tenantEm(this.em, organizationId);
      const entries = await em.find(SearchHistory, { organizationId, userId }, { orderBy: { createdAt: 'desc' } });
      history.push(
        ...entries.map((entry) => ({
          id: entry.id,
          ...(entry.query ? { query: entry.query } : {}),
          queryClass: entry.queryClass,
          channel: entry.channel,
          ...(entry.answerId ? { answerId: entry.answerId } : {}),
          createdAt: new Date(entry.createdAt).toISOString()
        }))
      );
    }
    return { SavedSearch: savedSearches, SearchHistory: history };
  }

  // Deleting needs no decryption, so it runs across organizations at once.
  async eraseUser(userId: string): Promise<{ entitiesAffected: string[]; recordsDeleted: number }> {
    const saved = await this.em.nativeDelete(SavedSearch, { userId });
    const history = await this.em.nativeDelete(SearchHistory, { userId });
    return {
      entitiesAffected: [...(saved > 0 ? ['SavedSearch'] : []), ...(history > 0 ? ['SearchHistory'] : [])],
      recordsDeleted: saved + history
    };
  }

  // Records a GET /search call (answers record their own history).
  async recordSearch(organizationId: string, userId: string, query: string | null, queryClass: string) {
    const em = tenantEm(this.em, organizationId);
    em.create(SearchHistory, { organizationId, userId, query, queryClass, channel: 'search', answerId: null });
    await em.flush();
  }
}

function view(saved: SavedSearch) {
  return {
    id: saved.id,
    name: saved.name,
    query: saved.query,
    ...(saved.topicSlug ? { topicSlug: saved.topicSlug } : {}),
    createdAt: new Date(saved.createdAt).toISOString()
  };
}

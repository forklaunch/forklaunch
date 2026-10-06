import { deanon, withEncryptionContext } from '@forklaunch/core/persistence';
import { EntityManager } from '@mikro-orm/core';
import { GeneratedAnswer } from '../../persistence/entities/generatedAnswer.entity';
import { SavedSearch } from '../../persistence/entities/savedSearch.entity';
import { SearchHistory } from '../../persistence/entities/searchHistory.entity';
import { organizationsOfUser } from '../tenants';
import { NO_ORGANIZATION_TENANT, tenantEm } from '../tenantEm';

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
    return entries.map(historyView);
  }

  // GDPR export and erasure for the user's data in MLSE. Implemented here
  // because ComplianceDataService in @forklaunch/core (2.1.0, still on
  // 3.0.1) cannot see the entities' compliance registrations, so it finds
  // nothing to export or erase.
  async exportUser(userId: string) {
    const savedSearches: ReturnType<typeof view>[] = [];
    const history: Awaited<ReturnType<SavedSearchService['history']>> = [];
    const answers: ReturnType<typeof answerView>[] = [];
    for (const organizationId of await organizationsOfUser(this.em, userId)) {
      const em = tenantEm(this.em, organizationId);
      if (organizationId !== NO_ORGANIZATION_TENANT) {
        savedSearches.push(...(await this.list(organizationId, userId)));
        const entries = await em.find(SearchHistory, { organizationId, userId }, { orderBy: { createdAt: 'desc' } });
        history.push(...entries.map(historyView));
      }
      const asked = await em.find(
        GeneratedAnswer,
        organizationId === NO_ORGANIZATION_TENANT ? { userId, organizationId: null } : { userId, organizationId },
        { orderBy: { createdAt: 'desc' } }
      );
      answers.push(...asked.map((a) => answerView(a, organizationId)));
    }
    return { SavedSearch: savedSearches, SearchHistory: history, GeneratedAnswer: answers };
  }

  // Deleting needs no decryption, so it runs across organizations at once.
  // Answer audit rows are kept for safety review, but lose the query text
  // and the link to the user.
  async eraseUser(userId: string): Promise<{ entitiesAffected: string[]; recordsDeleted: number }> {
    const saved = await this.em.nativeDelete(SavedSearch, { userId });
    const history = await this.em.nativeDelete(SearchHistory, { userId });
    const answers: { id: string }[] = await this.em.getConnection().execute(
      `update generated_answer set query = null, user_id = null, updated_at = now()
        where user_id = ? returning id`,
      [userId]
    );
    return {
      entitiesAffected: [
        ...(saved > 0 ? ['SavedSearch'] : []),
        ...(history > 0 ? ['SearchHistory'] : []),
        ...(answers.length > 0 ? ['GeneratedAnswer'] : [])
      ],
      recordsDeleted: saved + history + answers.length
    };
  }

  // Records a GET /search call (answers record their own history).
  async recordSearch(organizationId: string, userId: string, query: string | null, queryClass: string) {
    const em = tenantEm(this.em, organizationId);
    em.create(SearchHistory, { organizationId, userId, query, queryClass, channel: 'search', answerId: null });
    await em.flush();
  }
}

// name and query are encrypted (pii): core 3 loads them as CompliantField,
// and deanon() reads their plaintext, reporting each read to the access log.
// Decryption needs the row's organization bound at the moment of the read,
// not only during the query, so each row is read under its own.
function view(entity: SavedSearch) {
  const saved = withEncryptionContext(entity.organizationId, () => deanon(entity));
  return {
    id: saved.id,
    name: saved.name,
    query: saved.query,
    ...(saved.topicSlug ? { topicSlug: saved.topicSlug } : {}),
    createdAt: new Date(saved.createdAt).toISOString()
  };
}

function answerView(entity: GeneratedAnswer, tenant: string) {
  const answer = withEncryptionContext(tenant, () => deanon(entity));
  return {
    id: answer.id,
    ...(answer.query ? { query: answer.query } : {}),
    queryClass: answer.queryClass,
    kind: answer.kind,
    createdAt: new Date(answer.createdAt).toISOString()
  };
}

function historyView(entity: SearchHistory) {
  const entry = withEncryptionContext(entity.organizationId, () => deanon(entity));
  return {
    id: entry.id,
    // null for queries not stored (patient-specific, prescription,
    // emergency) and once the retention job has anonymized the entry
    ...(entry.query ? { query: entry.query } : {}),
    queryClass: entry.queryClass,
    channel: entry.channel,
    ...(entry.answerId ? { answerId: entry.answerId } : {}),
    createdAt: new Date(entry.createdAt).toISOString()
  };
}

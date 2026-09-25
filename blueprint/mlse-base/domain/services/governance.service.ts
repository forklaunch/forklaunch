import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { QUESTION_FRAMEWORKS } from '@forklaunch/implementation-mlse-base/services';
import { EntityManager } from '@mikro-orm/core';
import { ContentFlag } from '../../persistence/entities/contentFlag.entity';
import { ContentLicense } from '../../persistence/entities/contentLicense.entity';
import { Document } from '../../persistence/entities/document.entity';
import { DocumentChunk } from '../../persistence/entities/documentChunk.entity';
import { Source } from '../../persistence/entities/source.entity';
import { Topic } from '../../persistence/entities/topic.entity';

export class GovernanceNotFoundError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'GovernanceNotFoundError';
  }
}

export class GovernanceConflictError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'GovernanceConflictError';
  }
}

export type RegisterSourceRequest = {
  sourceKey: string;
  name: string;
  tier: string;
  licenseTerms: string;
  commercialUse: boolean;
};

export type CreateLicenseRequest = {
  organizationId: string;
  sourceKey: string;
  licensee: string;
  reference?: string;
  validFrom?: string;
  validUntil?: string;
  createdBy: string;
};

/**
 * Content governance: licensed sources and who may see them, reviewer flags
 * that take a document out of use, and topic approval.
 *
 * Every rule fails closed. A licensed source is visible to an organization
 * only with an active, in-date license; a flagged document stays hidden
 * until a reviewer decides; a topic can only be approved once its question
 * framework is clinician-approved.
 */
export class GovernanceService {
  constructor(
    private readonly em: EntityManager,
    private readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>
  ) {}

  // ContentSourceResolver: which sources an organization can search.
  async sourceAccess(organizationId: string | undefined) {
    const rows: { source_key: string; name: string; requires_license: boolean; licensed: boolean }[] =
      await this.em.getConnection().execute(
        `select s.source_key, s.name, s.requires_license,
                exists (select 1 from content_license l
                         where l.source_key = s.source_key and l.organization_id = ?
                           and l.status = 'active' and l.valid_from <= now()
                           and (l.valid_until is null or l.valid_until > now())) as licensed
           from source s order by s.source_key`,
        [organizationId ?? '']
      );
    return rows.map((row) => ({
      sourceKey: row.source_key,
      name: row.name,
      requiresLicense: row.requires_license,
      accessible: !row.requires_license || row.licensed
    }));
  }

  // A licensed source may be ingested only once registered as licensed and
  // while at least one organization holds an active license for it.
  async canIngest(sourceKey: string): Promise<boolean> {
    const source = await this.em.findOne(Source, { sourceKey });
    if (!source?.requiresLicense) {
      return false;
    }
    const rows: { active: number }[] = await this.em.getConnection().execute(
      `select count(*)::int as active from content_license
        where source_key = ? and status = 'active' and valid_from <= now()
          and (valid_until is null or valid_until > now())`,
      [sourceKey]
    );
    return (rows[0]?.active ?? 0) > 0;
  }

  async registerLicensedSource(request: RegisterSourceRequest) {
    const existing = await this.em.findOne(Source, { sourceKey: request.sourceKey });
    if (existing && !existing.requiresLicense) {
      throw new GovernanceConflictError(`Source '${request.sourceKey}' is a public source`);
    }
    const values = {
      name: request.name,
      tier: request.tier,
      licenseTerms: request.licenseTerms,
      commercialUse: request.commercialUse,
      liveQuery: false,
      requiresLicense: true
    };
    if (existing) {
      this.em.assign(existing, values);
    } else {
      this.em.create(Source, { sourceKey: request.sourceKey, lastRefreshedAt: null, ...values });
    }
    await this.em.flush();
    this.openTelemetryCollector.info('Licensed source registered', { sourceKey: request.sourceKey });
    return { sourceKey: request.sourceKey, requiresLicense: true };
  }

  async listLicenses(organizationId: string) {
    const licenses = await this.em.find(ContentLicense, { organizationId }, { orderBy: { createdAt: 'desc' } });
    return licenses.map(licenseView);
  }

  async createLicense(request: CreateLicenseRequest) {
    const source = await this.em.findOne(Source, { sourceKey: request.sourceKey });
    if (!source) {
      throw new GovernanceNotFoundError(`Source '${request.sourceKey}' not found`);
    }
    if (!source.requiresLicense) {
      throw new GovernanceConflictError(`Source '${request.sourceKey}' is public and needs no license`);
    }
    const validFrom = request.validFrom ? new Date(request.validFrom) : new Date();
    const validUntil = request.validUntil ? new Date(request.validUntil) : null;
    if (Number.isNaN(validFrom.getTime()) || (validUntil && Number.isNaN(validUntil.getTime()))) {
      throw new GovernanceConflictError('validFrom and validUntil must be ISO dates');
    }
    if (validUntil && validUntil <= validFrom) {
      throw new GovernanceConflictError('validUntil must be after validFrom');
    }
    const license = this.em.create(ContentLicense, {
      organizationId: request.organizationId,
      sourceKey: request.sourceKey,
      licensee: request.licensee,
      reference: request.reference ?? null,
      status: 'active',
      validFrom,
      validUntil,
      createdBy: request.createdBy
    });
    await this.em.flush();
    this.openTelemetryCollector.info('Content license created', {
      organizationId: request.organizationId,
      sourceKey: request.sourceKey
    });
    return licenseView(license);
  }

  async revokeLicense(organizationId: string, id: string) {
    const license = await this.em.findOne(ContentLicense, { id, organizationId });
    if (!license) {
      throw new GovernanceNotFoundError(`License '${id}' not found`);
    }
    license.status = 'revoked';
    await this.em.flush();
    return licenseView(license);
  }

  // Flags the document a passage belongs to, or a document by source and id.
  async flag(request: { passageId?: string; sourceKey?: string; externalId?: string; reason: string; flaggedBy: string }) {
    let document: Document | null = null;
    if (request.passageId) {
      const chunk = await this.em.findOne(DocumentChunk, { id: request.passageId }, { populate: ['document'] });
      document = (chunk?.document as unknown as Document | undefined) ?? null;
    } else if (request.sourceKey && request.externalId) {
      document = await this.em.findOne(
        Document,
        { sourceKey: request.sourceKey, externalId: request.externalId, status: 'current' }
      );
    }
    if (!document) {
      throw new GovernanceNotFoundError('No document matches the passage or source id given');
    }
    const flag = this.em.create(ContentFlag, {
      document,
      reason: request.reason,
      status: 'open',
      flaggedBy: request.flaggedBy,
      resolvedBy: null,
      resolution: null,
      resolvedAt: null
    });
    await this.em.flush();
    this.openTelemetryCollector.info('Content flagged', { flagId: flag.id, documentId: document.id });
    return this.flagView(flag, document);
  }

  async listFlags(status?: string) {
    const flags = await this.em.find(
      ContentFlag,
      status ? { status } : {},
      { populate: ['document'], orderBy: { createdAt: 'desc' } }
    );
    return flags.map((flag) => this.flagView(flag, flag.document as unknown as Document));
  }

  // outcome 'resolved': the document was corrected or removed; 'rejected':
  // the flag was unfounded. Either way the document is shown again unless
  // another flag is open.
  async resolveFlag(id: string, outcome: 'resolved' | 'rejected', resolvedBy: string, resolution: string) {
    const flag = await this.em.findOne(ContentFlag, { id }, { populate: ['document'] });
    if (!flag) {
      throw new GovernanceNotFoundError(`Flag '${id}' not found`);
    }
    if (flag.status !== 'open') {
      throw new GovernanceConflictError(`Flag '${id}' is already ${flag.status}`);
    }
    flag.status = outcome;
    flag.resolvedBy = resolvedBy;
    flag.resolution = resolution;
    flag.resolvedAt = new Date();
    await this.em.flush();
    return this.flagView(flag, flag.document as unknown as Document);
  }

  async approveTopic(slug: string, approvedBy: string) {
    const topic = await this.em.findOne(Topic, { slug });
    if (!topic) {
      throw new GovernanceNotFoundError(`Topic '${slug}' not found`);
    }
    const framework = QUESTION_FRAMEWORKS[topic.frameworkKey];
    if (!framework || framework.status !== 'approved') {
      throw new GovernanceConflictError(
        `Topic '${slug}' uses question framework '${topic.frameworkKey}', which is not clinician-approved yet`
      );
    }
    topic.status = 'approved';
    topic.approvedBy = approvedBy;
    topic.approvedAt = new Date();
    await this.em.flush();
    return { slug, status: topic.status, approvedBy };
  }

  private flagView(flag: ContentFlag, document: Document) {
    return {
      id: flag.id,
      sourceKey: document.sourceKey,
      externalId: document.externalId,
      title: document.title,
      reason: flag.reason,
      status: flag.status,
      flaggedBy: flag.flaggedBy,
      ...(flag.resolvedBy ? { resolvedBy: flag.resolvedBy } : {}),
      ...(flag.resolution ? { resolution: flag.resolution } : {}),
      createdAt: new Date(flag.createdAt).toISOString()
    };
  }
}

function licenseView(license: ContentLicense) {
  return {
    id: license.id,
    organizationId: license.organizationId,
    sourceKey: license.sourceKey,
    licensee: license.licensee,
    ...(license.reference ? { reference: license.reference } : {}),
    status: license.status,
    validFrom: new Date(license.validFrom).toISOString(),
    ...(license.validUntil ? { validUntil: new Date(license.validUntil).toISOString() } : {})
  };
}

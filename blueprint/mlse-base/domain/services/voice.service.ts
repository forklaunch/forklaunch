import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import {
  isValidCareArea,
  removeIdentifiers,
  spokenSummary
} from '@forklaunch/implementation-mlse-base/services';
import { AnswerResponseDto } from '@forklaunch/interfaces-mlse/types';
import { EntityManager } from '@mikro-orm/core';
import { VoiceSetting } from '../../persistence/entities/voiceSetting.entity';
import { countMetric } from '../metrics';
import { AnswerService } from './answer.service';

export class VoiceDisabledError extends Error {
  constructor(readonly area: string) {
    super(`Voice is not enabled for the '${area}' area of this organization`);
    this.name = 'VoiceDisabledError';
  }
}

export class InvalidCareAreaError extends Error {
  constructor(readonly area: string) {
    super(`'${area}' is not a valid care area (lowercase letters and underscores)`);
    this.name = 'InvalidCareAreaError';
  }
}

export type VoiceQueryRequest = {
  organizationId: string;
  area: string;
  transcript: string;
  userId?: string;
};

export type VoiceQueryResult = {
  spokenSummary: string;
  identifiersRemoved: number;
  answer: AnswerResponseDto;
};

/**
 * Voice search, off by default: a request is answered only if the
 * organization has explicitly enabled voice for the care area it comes
 * from. Anything else, including a missing setting or a failed lookup, is
 * refused. The transcript has identifiers removed before it is classified,
 * searched or stored.
 */
export class VoiceService {
  constructor(
    private readonly em: EntityManager,
    private readonly answerService: AnswerService,
    private readonly openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>
  ) {}

  async listSettings(organizationId: string) {
    const settings = await this.em.find(
      VoiceSetting,
      { organizationId },
      { orderBy: { area: 'asc' } }
    );
    return settings.map((s) => ({
      area: s.area,
      enabled: s.enabled,
      updatedBy: s.updatedBy,
      updatedAt: new Date(s.updatedAt).toISOString()
    }));
  }

  async setSetting(organizationId: string, area: string, enabled: boolean, updatedBy: string) {
    if (!isValidCareArea(area)) {
      throw new InvalidCareAreaError(area);
    }
    const existing = await this.em.findOne(VoiceSetting, { organizationId, area });
    if (existing) {
      existing.enabled = enabled;
      existing.updatedBy = updatedBy;
    } else {
      this.em.create(VoiceSetting, { organizationId, area, enabled, updatedBy });
    }
    await this.em.flush();
    this.openTelemetryCollector.info('Voice setting changed', { organizationId, area, enabled, updatedBy });
    return { area, enabled, updatedBy };
  }

  async isEnabled(organizationId: string, area: string): Promise<boolean> {
    if (!isValidCareArea(area)) {
      return false;
    }
    try {
      const setting = await this.em.findOne(VoiceSetting, { organizationId, area });
      return setting?.enabled === true;
    } catch (error) {
      // fail closed: if the setting cannot be read, voice stays off
      this.openTelemetryCollector.error('Voice setting lookup failed', {
        organizationId,
        area,
        error: error instanceof Error ? error.message : String(error)
      });
      return false;
    }
  }

  async query(request: VoiceQueryRequest): Promise<VoiceQueryResult> {
    if (!(await this.isEnabled(request.organizationId, request.area))) {
      countMetric(this.openTelemetryCollector, 'mlse_voice_requests_total', 1, { outcome: 'voice_disabled' });
      throw new VoiceDisabledError(request.area);
    }
    const cleaned = removeIdentifiers(request.transcript);
    const answer = await this.answerService.answer({
      query: cleaned.text,
      organizationId: request.organizationId,
      channel: 'voice',
      ...(request.userId ? { userId: request.userId } : {})
    });
    countMetric(this.openTelemetryCollector, 'mlse_voice_requests_total', 1, { outcome: 'answered' });
    return {
      spokenSummary: spokenSummary(answer),
      identifiersRemoved: cleaned.removed,
      answer
    };
  }
}

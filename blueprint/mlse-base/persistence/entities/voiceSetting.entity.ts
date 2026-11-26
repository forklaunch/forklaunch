import { sqlBaseProperties } from '@forklaunch/blueprint-core';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';
import type { InferEntity } from '@mikro-orm/core';

/**
 * Whether voice search may be used in one care area of one organization,
 * for example operating_room on and emergency off. Voice is off wherever no
 * row says otherwise: whether microphones are acceptable in an area is the
 * hospital's policy and legal decision, recorded here with who made it.
 */
export const VoiceSetting = defineComplianceEntity({
  name: 'VoiceSetting',
  properties: {
    ...sqlBaseProperties,
    organizationId: fp.string().compliance('none'),
    area: fp.string().compliance('none'),
    enabled: fp.boolean().default(false).compliance('none'),
    updatedBy: fp.string().compliance('none')
  }
});

export type VoiceSetting = InferEntity<typeof VoiceSetting>;

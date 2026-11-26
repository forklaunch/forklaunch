import { ResolvedEntity } from '@forklaunch/core/persistence';
import { SmsRecord } from '../../persistence/entities';

// sms record entity types
export type BaseSmsEntities = {
  SmsRecordMapper: {
    '~entity': ResolvedEntity<(typeof SmsRecord)['~entity']>;
  };
  SendSmsMapper: {
    '~entity': ResolvedEntity<(typeof SmsRecord)['~entity']>;
  };
};

import { EntityManager, InferEntity } from '@mikro-orm/core';
import { BaseSmsDtos } from './baseSmsDto.types';
import { BaseSmsEntities } from './baseSmsEntity.types';

export type SmsMappers<
  StatusEnum,
  MapperEntities extends BaseSmsEntities,
  MapperDomains extends BaseSmsDtos<StatusEnum>
> = {
  SmsRecordMapper: {
    entity: MapperEntities['SmsRecordMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['SmsRecordMapper']>
    ) => Promise<MapperDomains['SmsRecordMapper']>;
  };
  SendSmsMapper: {
    entity: MapperEntities['SendSmsMapper'];
    toEntity: (
      dto: MapperDomains['SendSmsMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['SendSmsMapper']>>;
  };
};

import { EntityManager, InferEntity } from '@mikro-orm/core';
import { UserDtos } from './iamDto.types';
import { UserEntities } from './iamEntities.types';

export type UserMappers<
  MapperEntities extends UserEntities,
  MapperDomains extends UserDtos
> = {
  UserMapper: {
    entity: MapperEntities['UserMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['UserMapper']>
    ) => Promise<MapperDomains['UserMapper']>;
  };
  CreateUserMapper: {
    entity: MapperEntities['CreateUserMapper'];
    toEntity: (
      dto: MapperDomains['CreateUserMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateUserMapper']>>;
  };
  UpdateUserMapper: {
    entity: MapperEntities['UpdateUserMapper'];
    toEntity: (
      dto: MapperDomains['UpdateUserMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateUserMapper']>>;
  };
};

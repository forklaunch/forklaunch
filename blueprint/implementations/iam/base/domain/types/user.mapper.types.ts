import { EntityManager, EntitySchema } from '@mikro-orm/core';
import { UserDtos } from './iamDto.types';
import { UserEntities } from './iamEntities.types';

export type UserMappers<
  MapperEntities extends UserEntities,
  MapperDomains extends UserDtos
> = {
  UserMapper: {
    entity: EntitySchema<MapperEntities['UserMapper']>;
    toDto: (
      entity: MapperEntities['UserMapper']
    ) => Promise<MapperDomains['UserMapper']>;
  };
  CreateUserMapper: {
    entity: EntitySchema<MapperEntities['UserMapper']>;
    toEntity: (
      dto: MapperDomains['CreateUserMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['CreateUserMapper']>;
  };
  UpdateUserMapper: {
    entity: EntitySchema<MapperEntities['UserMapper']>;
    toEntity: (
      dto: MapperDomains['UpdateUserMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['UpdateUserMapper']>;
  };
};

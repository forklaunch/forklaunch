import { EntityManager, EntitySchema } from '@mikro-orm/core';
import { RoleDtos } from './iamDto.types';
import { RoleEntities } from './iamEntities.types';

export type RoleMappers<
  MapperEntities extends RoleEntities,
  MapperDomains extends RoleDtos
> = {
  RoleMapper: {
    entity: EntitySchema<any>;
    toDto: (
      entity: MapperEntities['RoleMapper']
    ) => Promise<MapperDomains['RoleMapper']>;
  };
  CreateRoleMapper: {
    entity: EntitySchema<any>;
    toEntity: (
      dto: MapperDomains['CreateRoleMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['CreateRoleMapper']>;
  };
  UpdateRoleMapper: {
    entity: EntitySchema<any>;
    toEntity: (
      dto: MapperDomains['UpdateRoleMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['UpdateRoleMapper']>;
  };
};

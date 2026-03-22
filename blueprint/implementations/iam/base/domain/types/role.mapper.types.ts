import { EntityManager, InferEntity } from '@mikro-orm/core';
import { RoleDtos } from './iamDto.types';
import { RoleEntities } from './iamEntities.types';

export type RoleMappers<
  MapperEntities extends RoleEntities,
  MapperDomains extends RoleDtos
> = {
  RoleMapper: {
    entity: MapperEntities['RoleMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['RoleMapper']>
    ) => Promise<MapperDomains['RoleMapper']>;
  };
  CreateRoleMapper: {
    entity: MapperEntities['CreateRoleMapper'];
    toEntity: (
      dto: MapperDomains['CreateRoleMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateRoleMapper']>>;
  };
  UpdateRoleMapper: {
    entity: MapperEntities['UpdateRoleMapper'];
    toEntity: (
      dto: MapperDomains['UpdateRoleMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateRoleMapper']>>;
  };
};

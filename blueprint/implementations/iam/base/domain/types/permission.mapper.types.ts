import { EntityManager, EntitySchema } from '@mikro-orm/core';
import { PermissionDtos } from './iamDto.types';
import { PermissionEntities } from './iamEntities.types';

export type PermissionMappers<
  MapperEntities extends PermissionEntities,
  MapperDomains extends PermissionDtos
> = {
  PermissionMapper: {
    entity: EntitySchema<any>;
    toDto: (
      entity: MapperEntities['PermissionMapper']
    ) => Promise<MapperDomains['PermissionMapper']>;
  };
  CreatePermissionMapper: {
    entity: EntitySchema<any>;
    toEntity: (
      dto: MapperDomains['CreatePermissionMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['CreatePermissionMapper']>;
  };
  UpdatePermissionMapper: {
    entity: EntitySchema<any>;
    toEntity: (
      dto: MapperDomains['UpdatePermissionMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['UpdatePermissionMapper']>;
  };
  RoleEntityMapper: {
    entity: EntitySchema<any>;
    toEntity: (
      dto: MapperDomains['RoleEntityMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['RoleEntityMapper']>;
  };
};

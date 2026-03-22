import { EntityManager, InferEntity } from '@mikro-orm/core';
import { PermissionDtos } from './iamDto.types';
import { PermissionEntities } from './iamEntities.types';

export type PermissionMappers<
  MapperEntities extends PermissionEntities,
  MapperDomains extends PermissionDtos
> = {
  PermissionMapper: {
    entity: MapperEntities['PermissionMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['PermissionMapper']>
    ) => Promise<MapperDomains['PermissionMapper']>;
  };
  CreatePermissionMapper: {
    entity: MapperEntities['CreatePermissionMapper'];
    toEntity: (
      dto: MapperDomains['CreatePermissionMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreatePermissionMapper']>>;
  };
  UpdatePermissionMapper: {
    entity: MapperEntities['UpdatePermissionMapper'];
    toEntity: (
      dto: MapperDomains['UpdatePermissionMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdatePermissionMapper']>>;
  };
  RoleEntityMapper: {
    entity: MapperEntities['RoleEntityMapper'];
    toEntity: (
      dto: MapperDomains['RoleEntityMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['RoleEntityMapper']>>;
  };
};

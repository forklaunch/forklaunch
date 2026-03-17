import { EntityManager, EntitySchema } from '@mikro-orm/core';
import { OrganizationDtos } from './iamDto.types';
import { OrganizationEntities } from './iamEntities.types';

export type OrganizationMappers<
  OrganizationStatus,
  MapperEntities extends OrganizationEntities<OrganizationStatus>,
  MapperDomains extends OrganizationDtos<OrganizationStatus>
> = {
  OrganizationMapper: {
    entity: EntitySchema<any>;
    toDto: (
      entity: MapperEntities['OrganizationMapper']
    ) => Promise<MapperDomains['OrganizationMapper']>;
  };
  CreateOrganizationMapper: {
    entity: EntitySchema<any>;
    toEntity: (
      dto: MapperDomains['CreateOrganizationMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['CreateOrganizationMapper']>;
  };
  UpdateOrganizationMapper: {
    entity: EntitySchema<any>;
    toEntity: (
      dto: MapperDomains['UpdateOrganizationMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<MapperEntities['UpdateOrganizationMapper']>;
  };
};

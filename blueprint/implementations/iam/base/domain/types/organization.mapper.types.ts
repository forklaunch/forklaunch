import { EntityManager, InferEntity } from '@mikro-orm/core';
import { OrganizationDtos } from './iamDto.types';
import { OrganizationEntities } from './iamEntities.types';

export type OrganizationMappers<
  OrganizationStatus,
  MapperEntities extends OrganizationEntities<OrganizationStatus>,
  MapperDomains extends OrganizationDtos<OrganizationStatus>
> = {
  OrganizationMapper: {
    entity: MapperEntities['OrganizationMapper'];
    toDto: (
      entity: InferEntity<MapperEntities['OrganizationMapper']>
    ) => Promise<MapperDomains['OrganizationMapper']>;
  };
  CreateOrganizationMapper: {
    entity: MapperEntities['CreateOrganizationMapper'];
    toEntity: (
      dto: MapperDomains['CreateOrganizationMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['CreateOrganizationMapper']>>;
  };
  UpdateOrganizationMapper: {
    entity: MapperEntities['UpdateOrganizationMapper'];
    toEntity: (
      dto: MapperDomains['UpdateOrganizationMapper'],
      em: EntityManager,
      ...args: unknown[]
    ) => Promise<InferEntity<MapperEntities['UpdateOrganizationMapper']>>;
  };
};

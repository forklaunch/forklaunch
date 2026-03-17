import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import { OrganizationStatus } from '../../domain/enum/organizationStatus.enum';
import {
  organization,
  type Organization
} from '../../persistence/entities/organization.entity';
import { OrganizationSchemas } from '../schemas';
import { UserMapper } from './user.mappers';

export const CreateOrganizationMapper = requestMapper({
  schemaValidator,
  schema: OrganizationSchemas.CreateOrganizationSchema,
  entity: organization,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(organization, {
        name: dto.name,
        domain: dto.domain,
        subscription: dto.subscription,
        logoUrl: dto.logoUrl || null,
        users: [],
        status: OrganizationStatus.ACTIVE,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdateOrganizationMapper = requestMapper({
  schemaValidator,
  schema: OrganizationSchemas.UpdateOrganizationSchema,
  entity: organization,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(organization, { id: dto.id });
      em.assign(entity, {
        ...(dto.name !== undefined && { name: dto.name }),
        ...(dto.domain !== undefined && { domain: dto.domain }),
        ...(dto.subscription !== undefined && {
          subscription: dto.subscription
        }),
        ...(dto.logoUrl !== undefined && { logoUrl: dto.logoUrl }),
        updatedAt: new Date()
      });
      return entity;
    }
  }
});

export const OrganizationMapper = responseMapper({
  schemaValidator,
  schema: OrganizationSchemas.OrganizationSchema(OrganizationStatus),
  entity: organization,
  mapperDefinition: {
    toDto: async (entity: Organization) => {
      return {
        ...wrap(entity).toPOJO(),
        users: await Promise.all(
          (entity.users.isInitialized()
            ? entity.users
            : await entity.users.init()
          )
            .getItems()
            .map(async (user) => UserMapper.toDto(user))
        )
      };
    }
  }
});

import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import {
  Organization,
  type IOrganization
} from '../../persistence/entities/organization.entity';
import { OrganizationStatus } from '../enum/organizationStatus.enum';
import { OrganizationSchemas } from '../schemas';
import { UserMapper } from './user.mappers';

export const CreateOrganizationMapper = requestMapper({
  schemaValidator,
  schema: OrganizationSchemas.CreateOrganizationSchema,
  entity: Organization,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Organization, {
        ...dto,
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
  entity: Organization,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(Organization, { id: dto.id });
      em.assign(entity, { ...dto, updatedAt: new Date() });
      return entity;
    }
  }
});

export const OrganizationMapper = responseMapper({
  schemaValidator,
  schema: OrganizationSchemas.OrganizationSchema(OrganizationStatus),
  entity: Organization,
  mapperDefinition: {
    toDto: async (entity: IOrganization) => {
      const pojo = wrap(entity).toPOJO();
      return {
        ...pojo,
        logoUrl: pojo.logoUrl || undefined,
        users: await Promise.all(
          (entity.users.isInitialized()
            ? entity.users
            : await entity.users.init()
          )
            .getItems()
            .map(async (user) => {
              return UserMapper.toDto(user);
            })
        )
      };
    }
  }
});

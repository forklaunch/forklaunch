import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { RoleService } from '@forklaunch/interfaces-iam/interfaces';
import { EntityManager } from '@mikro-orm/core';

import { IdDto, IdsDto } from '@forklaunch/common';
import {
  CreateRoleDto,
  RoleDto,
  UpdateRoleDto
} from '@forklaunch/interfaces-iam/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { RoleDtos } from '../domain/types/iamDto.types';
import { RoleEntities } from '../domain/types/iamEntities.types';
import { RoleMappers } from '../domain/types/role.mapper.types';

export class BaseRoleService<
  SchemaValidator extends AnySchemaValidator,
  MapperEntities extends RoleEntities = RoleEntities,
  MapperDomains extends RoleDtos = RoleDtos
> implements RoleService
{
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: RoleMappers<MapperEntities, MapperDomains>;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: RoleMappers<MapperEntities, MapperDomains>,
    options?: {
      telemetry?: TelemetryOptions;
    }
  ) {
    this.em = em;
    this.openTelemetryCollector = openTelemetryCollector;
    this.schemaValidator = schemaValidator;
    this.mappers = mappers;
    this.evaluatedTelemetryOptions = options?.telemetry
      ? evaluateTelemetryOptions(options.telemetry).enabled
      : {
          logging: false,
          metrics: false,
          tracing: false
        };
  }

  async createRole(
    roleDto: CreateRoleDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<RoleDto> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Creating role', roleDto);
    }
    const role = await this.mappers.CreateRoleMapper.toEntity(
      roleDto,
      em ?? this.em,
      ...args
    );

    if (em) {
      await em.persist(role);
    } else {
      await this.em.persist(role).flush();
    }

    return this.mappers.RoleMapper.toDto(role);
  }

  async createBatchRoles(
    roleDtos: CreateRoleDto[],
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<RoleDto[]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Creating batch roles', roleDtos);
    }

    const roles = await Promise.all(
      roleDtos.map(async (roleDto) =>
        this.mappers.CreateRoleMapper.toEntity(roleDto, em ?? this.em, ...args)
      )
    );

    if (em) {
      await em.persist(roles);
    } else {
      await this.em.persist(roles).flush();
    }

    return Promise.all(
      roles.map((role) => this.mappers.RoleMapper.toDto(role))
    );
  }

  async getRole({ id }: IdDto, em?: EntityManager): Promise<RoleDto> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting role', { id });
    }

    const role = await (em ?? this.em).findOneOrFail(
      this.mappers.RoleMapper.entity,
      id,
      {
        populate: ['id', '*']
      }
    );

    return this.mappers.RoleMapper.toDto(role);
  }

  async getBatchRoles({ ids }: IdsDto, em?: EntityManager): Promise<RoleDto[]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting batch roles', { ids });
    }

    return Promise.all(
      (
        await (em ?? this.em).find(
          this.mappers.RoleMapper.entity,
          {
            id: { $in: ids }
          },
          {
            populate: ['id', '*']
          }
        )
      ).map((role) => this.mappers.RoleMapper.toDto(role))
    );
  }

  async updateRole(
    roleDto: UpdateRoleDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<RoleDto> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Updating role', roleDto);
    }

    const role = await this.mappers.UpdateRoleMapper.toEntity(
      roleDto,
      em ?? this.em,
      ...args
    );

    if (em) {
      await em.persist(role);
    } else {
      await this.em.persist(role).flush();
    }

    return this.mappers.RoleMapper.toDto(role);
  }

  async updateBatchRoles(
    roleDtos: UpdateRoleDto[],
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<RoleDto[]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Updating batch roles', roleDtos);
    }

    const roles = await Promise.all(
      roleDtos.map(async (roleDto) =>
        this.mappers.UpdateRoleMapper.toEntity(roleDto, em ?? this.em, ...args)
      )
    );

    if (em) {
      await em.persist(roles);
    } else {
      await this.em.persist(roles).flush();
    }
    return Promise.all(
      roles.map((role) => this.mappers.RoleMapper.toDto(role))
    );
  }

  async deleteRole(idDto: IdDto, em?: EntityManager): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Deleting role', idDto);
    }

    await (em ?? this.em).nativeDelete(this.mappers.RoleMapper.entity, idDto);
  }

  async deleteBatchRoles(idsDto: IdsDto, em?: EntityManager): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Deleting batch roles', idsDto);
    }

    await (em ?? this.em).nativeDelete(this.mappers.RoleMapper.entity, {
      id: { $in: idsDto.ids }
    });
  }
}

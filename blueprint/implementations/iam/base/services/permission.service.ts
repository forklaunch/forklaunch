import {
  PermissionService,
  RoleService
} from '@forklaunch/interfaces-iam/interfaces';

import { IdDto, IdsDto } from '@forklaunch/common';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import {
  CreatePermissionDto,
  UpdatePermissionDto
} from '@forklaunch/interfaces-iam/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import { PermissionDtos } from '../domain/types/iamDto.types';
import { PermissionEntities } from '../domain/types/iamEntities.types';
import { PermissionMappers } from '../domain/types/permission.mapper.types';

export class BasePermissionService<
  SchemaValidator extends AnySchemaValidator,
  MapperEntities extends PermissionEntities = PermissionEntities,
  MapperDomains extends PermissionDtos = PermissionDtos
> implements PermissionService
{
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected roleServiceFactory: () => RoleService;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: PermissionMappers<MapperEntities, MapperDomains>;

  constructor(
    em: EntityManager,
    roleServiceFactory: () => RoleService,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: PermissionMappers<MapperEntities, MapperDomains>,
    options?: {
      telemetry?: TelemetryOptions;
    }
  ) {
    this.em = em;
    this.roleServiceFactory = roleServiceFactory;
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

  // start: global helper functions
  private async updateRolesWithPermissions(
    roles: MapperEntities['RoleEntityMapper'][],
    permissions: MapperEntities['PermissionMapper'][]
  ): Promise<MapperEntities['RoleEntityMapper'][]> {
    return Promise.all(
      roles.map(async (role) => {
        permissions.forEach((permission) => role.permissions.add(permission));
        return role;
      })
    );
  }

  private async removePermissionsFromRoles(
    roles: MapperEntities['RoleEntityMapper'][],
    permissions: MapperEntities['PermissionMapper'][]
  ): Promise<MapperEntities['RoleEntityMapper'][]> {
    return Promise.all(
      roles.map(async (role) => {
        permissions.forEach((permission) =>
          role.permissions.remove(permission)
        );
        return role;
      })
    );
  }

  private async getBatchRoles(
    roles?: IdsDto,
    em?: EntityManager
  ): Promise<MapperEntities['RoleEntityMapper'][]> {
    return roles
      ? await Promise.all(
          (await this.roleServiceFactory().getBatchRoles(roles, em)).map(
            async (role) => {
              return (em ?? this.em).merge(
                await this.mappers.RoleEntityMapper.toEntity(
                  role,
                  em ?? this.em
                )
              );
            }
          )
        )
      : [];
  }
  // end: global helper functions

  // start: createPermission helper functions
  private async createPermissionEntity({
    permission,
    addToRoles
  }: {
    permission: MapperEntities['PermissionMapper'];
    addToRoles: MapperEntities['RoleEntityMapper'][];
  }): Promise<{
    permission: MapperEntities['PermissionMapper'];
    roles: MapperEntities['RoleEntityMapper'][];
  }> {
    let roles: MapperEntities['RoleEntityMapper'][] = [];
    if (addToRoles) {
      roles = await this.updateRolesWithPermissions(addToRoles, [permission]);
    }

    return { permission, roles };
  }

  private async extractCreatePermissionEntityToEntityData(
    permissionDto: CreatePermissionDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<{
    permission: MapperEntities['PermissionMapper'];
    addToRoles: MapperEntities['RoleEntityMapper'][];
  }> {
    return {
      permission: (em ?? this.em).merge(
        await this.mappers.CreatePermissionMapper.toEntity(
          permissionDto,
          em ?? this.em,
          ...args
        )
      ),
      addToRoles: permissionDto.addToRolesIds
        ? await this.getBatchRoles({ ids: permissionDto.addToRolesIds }, em)
        : []
    };
  }
  // end: createPermission helper functions

  async createPermission(
    createPermissionEntity: CreatePermissionDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['PermissionMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Creating permission',
        createPermissionEntity
      );
    }
    const { permission, roles } = await this.createPermissionEntity(
      await this.extractCreatePermissionEntityToEntityData(
        createPermissionEntity,
        em,
        ...args
      )
    );

    if (em) {
      await em.persist([permission, ...roles]);
    } else {
      await this.em.persist([permission, ...roles]).flush();
    }

    return this.mappers.PermissionMapper.toDto(permission);
  }

  async createBatchPermissions(
    permissionDtos: CreatePermissionDto[],
    em?: EntityManager
  ): Promise<MapperDomains['PermissionMapper'][]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Creating batch permissions',
        permissionDtos
      );
    }
    const rolesCache: Record<string, MapperEntities['RoleEntityMapper']> = {};
    const permissions: MapperEntities['PermissionMapper'][] = [];
    for (const createPermissionEntity of permissionDtos) {
      const { permission, roles } = await this.createPermissionEntity(
        await this.extractCreatePermissionEntityToEntityData(
          createPermissionEntity,
          em
        )
      );
      await Promise.all(
        roles.map(async (role) => {
          if (!role.permissions.isInitialized()) {
            return role.permissions.init();
          }
        })
      );
      await Promise.all(
        roles.map(async (role) => {
          if (!role.permissions.isInitialized()) {
            return role.permissions.init();
          }
        })
      );
      roles.forEach((role) => {
        if (
          rolesCache[role.id] &&
          role.permissions !== rolesCache[role.id].permissions
        ) {
          role.permissions.getItems().forEach((permission) => {
            if (!rolesCache[role.id].permissions.contains(permission)) {
              rolesCache[role.id].permissions.add(permission);
            }
          });
        } else {
          rolesCache[role.id] = role;
        }
      });
      permissions.push(permission);
    }
    const entities = [...permissions, ...Object.values(rolesCache)];

    if (em) {
      await em.persist(entities);
    } else {
      await this.em.persist(entities).flush();
    }

    return Promise.all(
      permissions.map(async (permission) =>
        this.mappers.PermissionMapper.toDto(permission)
      )
    );
  }

  async getPermission(
    idDto: IdDto,
    em?: EntityManager
  ): Promise<MapperDomains['PermissionMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting permission', idDto);
    }
    const permission = await (em ?? this.em).findOneOrFail(
      this.mappers.PermissionMapper.entity,
      idDto
    );
    return this.mappers.PermissionMapper.toDto(
      permission as MapperEntities['PermissionMapper']
    );
  }

  async getBatchPermissions(
    idsDto: IdsDto,
    em?: EntityManager
  ): Promise<MapperDomains['PermissionMapper'][]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting batch permissions', idsDto);
    }
    return Promise.all(
      (
        await (em ?? this.em).find(this.mappers.PermissionMapper.entity, {
          id: { $in: idsDto.ids }
        })
      ).map((permission) =>
        this.mappers.PermissionMapper.toDto(
          permission as MapperEntities['PermissionMapper']
        )
      )
    );
  }

  // start: updatePermission helper functions
  private updatePermissionDto = async (
    permissionDto: UpdatePermissionDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<{
    permission: MapperEntities['PermissionMapper'];
    roles: MapperEntities['RoleEntityMapper'][];
  }> => {
    const permission = await this.mappers.UpdatePermissionMapper.toEntity(
      permissionDto,
      em ?? this.em,
      ...args
    );
    const addToRoles = permissionDto.addToRolesIds
      ? await this.getBatchRoles({ ids: permissionDto.addToRolesIds }, em)
      : [];
    const removeFromRoles = permissionDto.removeFromRolesIds
      ? await this.getBatchRoles({ ids: permissionDto.removeFromRolesIds }, em)
      : [];

    let roles: MapperEntities['RoleEntityMapper'][] = [];

    roles = roles.concat(
      await this.updateRolesWithPermissions(addToRoles, [permission])
    );
    roles = roles.concat(
      await this.removePermissionsFromRoles(removeFromRoles, [permission])
    );

    return {
      permission,
      roles
    };
  };
  // end: updatePermission helper functions

  async updatePermission(
    permissionDto: UpdatePermissionDto,
    em?: EntityManager
  ): Promise<MapperDomains['PermissionMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Updating permission', permissionDto);
    }
    const { permission, roles } = await this.updatePermissionDto(permissionDto);
    const entities = await (em ?? this.em).upsertMany([permission, ...roles]);

    if (em) {
      await em.persist(entities);
    } else {
      await this.em.persist(entities).flush();
    }

    return this.mappers.PermissionMapper.toDto(permission);
  }

  async updateBatchPermissions(
    permissionDtos: UpdatePermissionDto[],
    em?: EntityManager
  ): Promise<MapperDomains['PermissionMapper'][]> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Updating batch permissions',
        permissionDtos
      );
    }
    const rolesCache: Record<string, MapperEntities['RoleEntityMapper']> = {};
    const permissions: MapperEntities['PermissionMapper'][] = [];
    await (em ?? this.em).transactional(async (em) => {
      permissionDtos.map(async (updatePermissionDto) => {
        const { permission, roles } =
          await this.updatePermissionDto(updatePermissionDto);
        roles.forEach((role) => {
          if (
            rolesCache[role.id] &&
            role.permissions !== rolesCache[role.id].permissions
          ) {
            role.permissions.getItems().forEach((permission) => {
              if (!rolesCache[role.id].permissions.contains(permission)) {
                rolesCache[role.id].permissions.add(permission);
              }
            });
          } else {
            rolesCache[role.id] = role;
          }
        });
        permissions.push(permission);
      });
      const entities = [...permissions, ...Object.values(rolesCache)];

      if (em) {
        await em.persist(entities);
      } else {
        await this.em.persist(entities).flush();
      }
    });

    return Promise.all(
      permissions.map((permission) =>
        this.mappers.PermissionMapper.toDto(permission)
      )
    );
  }

  async deletePermission(idDto: IdDto, em?: EntityManager): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Deleting permission', idDto);
    }
    await (em ?? this.em).nativeDelete(
      this.mappers.PermissionMapper.entity,
      idDto
    );
  }

  async deleteBatchPermissions(
    idsDto: IdsDto,
    em?: EntityManager
  ): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Deleting batch permissions', idsDto);
    }
    await (em ?? this.em).nativeDelete(this.mappers.PermissionMapper.entity, {
      id: { $in: idsDto.ids }
    });
  }
}

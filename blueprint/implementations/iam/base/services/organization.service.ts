import { IdDto } from '@forklaunch/common';
import {
  evaluateTelemetryOptions,
  MetricsDefinition,
  OpenTelemetryCollector,
  TelemetryOptions
} from '@forklaunch/core/http';
import { OrganizationService } from '@forklaunch/interfaces-iam/interfaces';
import {
  CreateOrganizationDto,
  UpdateOrganizationDto
} from '@forklaunch/interfaces-iam/types';
import { AnySchemaValidator } from '@forklaunch/validator';
import { EntityManager } from '@mikro-orm/core';
import { OrganizationDtos } from '../domain/types/iamDto.types';
import { OrganizationEntities } from '../domain/types/iamEntities.types';
import { OrganizationMappers } from '../domain/types/organization.mapper.types';

export class BaseOrganizationService<
  SchemaValidator extends AnySchemaValidator,
  OrganizationStatus = unknown,
  MapperEntities extends
    OrganizationEntities<OrganizationStatus> = OrganizationEntities<OrganizationStatus>,
  MapperDomains extends
    OrganizationDtos<OrganizationStatus> = OrganizationDtos<OrganizationStatus>
> implements OrganizationService<OrganizationStatus>
{
  // protected _mappers: InternalMapper<InstanceTypeRecord<typeof this.mappers>>;
  private evaluatedTelemetryOptions: {
    logging?: boolean;
    metrics?: boolean;
    tracing?: boolean;
  };
  public em: EntityManager;
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;
  protected schemaValidator: SchemaValidator;
  protected mappers: OrganizationMappers<
    OrganizationStatus,
    MapperEntities,
    MapperDomains
  >;

  constructor(
    em: EntityManager,
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>,
    schemaValidator: SchemaValidator,
    mappers: OrganizationMappers<
      OrganizationStatus,
      MapperEntities,
      MapperDomains
    >,
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

  async createOrganization(
    organizationDto: CreateOrganizationDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['OrganizationMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Creating organization',
        organizationDto
      );
    }

    const organization = await this.mappers.CreateOrganizationMapper.toEntity(
      organizationDto,
      em ?? this.em,
      ...args
    );

    if (em) {
      await em.persist(organization);
    } else {
      await this.em.persist(organization).flush();
    }

    return this.mappers.OrganizationMapper.toDto(organization);
  }

  async getOrganization(
    idDto: IdDto,
    em?: EntityManager
  ): Promise<MapperDomains['OrganizationMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Getting organization', idDto);
    }

    const organization = await (em ?? this.em).findOneOrFail(
      this.mappers.OrganizationMapper.entity,
      idDto,
      {
        populate: ['id', '*']
      }
    );

    return this.mappers.OrganizationMapper.toDto(
      organization as MapperEntities['OrganizationMapper']
    );
  }

  async updateOrganization(
    organizationDto: UpdateOrganizationDto,
    em?: EntityManager,
    ...args: unknown[]
  ): Promise<MapperDomains['OrganizationMapper']> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info(
        'Updating organization',
        organizationDto
      );
    }

    const updatedOrganization =
      await this.mappers.UpdateOrganizationMapper.toEntity(
        organizationDto,
        em ?? this.em,
        ...args
      );

    if (em) {
      await em.persist(updatedOrganization);
    } else {
      await this.em.persist(updatedOrganization).flush();
    }

    return this.mappers.OrganizationMapper.toDto(updatedOrganization);
  }

  async deleteOrganization(idDto: IdDto, em?: EntityManager): Promise<void> {
    if (this.evaluatedTelemetryOptions.logging) {
      this.openTelemetryCollector.info('Deleting organization', idDto);
    }

    if (em) {
      await em.nativeDelete(this.mappers.OrganizationMapper.entity, idDto);
    } else {
      await this.em.nativeDelete(this.mappers.OrganizationMapper.entity, idDto);
    }
  }
}

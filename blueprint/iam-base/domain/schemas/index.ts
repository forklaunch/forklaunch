import { SchemaValidator } from '@forklaunch/validator/typebox';
const schemaValidator = SchemaValidator();
type SchemaValidator = ReturnType<typeof SchemaValidator>;
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import {
  BaseOrganizationServiceSchemas,
  BasePermissionServiceSchemas,
  BaseRoleServiceSchemas,
  BaseUserServiceSchemas
} from '@forklaunch/implementation-iam-base/schemas';

const schemas = mapServiceSchemas(
  {
    OrganizationSchemas: BaseOrganizationServiceSchemas<SchemaValidator>,
    PermissionSchemas: BasePermissionServiceSchemas<SchemaValidator>,
    RoleSchemas: BaseRoleServiceSchemas<SchemaValidator>,
    UserSchemas: BaseUserServiceSchemas<SchemaValidator>
  },
  {
    uuidId: true,
    validator: schemaValidator
  }
);

export const {
  OrganizationSchemas,
  PermissionSchemas,
  RoleSchemas,
  UserSchemas
} = schemas;

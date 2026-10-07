import { isTrue } from '@forklaunch/common';
import {
  CreateOrganizationDto,
  CreatePermissionDto,
  CreateRoleDto,
  CreateUserDto,
  OrganizationDto,
  PermissionDto,
  RoleDto,
  UpdateOrganizationDto,
  UpdatePermissionDto,
  UpdateRoleDto,
  UpdateUserDto,
  UserDto
} from '@forklaunch/interfaces-iam/types';
import { DummyEnum, testSchemaEquality } from '@forklaunch/internal';
import {
  CreateOrganizationSchema as TypeboxCreateOrganizationSchema,
  OrganizationSchema as TypeboxOrganizationSchema,
  UpdateOrganizationSchema as TypeboxUpdateOrganizationSchema
} from '../domain/schemas/typebox/organization.schema';
import {
  CreatePermissionSchema as TypeboxCreatePermissionSchema,
  PermissionSchema as TypeboxPermissionSchema,
  UpdatePermissionSchema as TypeboxUpdatePermissionSchema
} from '../domain/schemas/typebox/permission.schema';
import {
  CreateRoleSchema as TypeboxCreateRoleSchema,
  RoleSchema as TypeboxRoleSchema,
  UpdateRoleSchema as TypeboxUpdateRoleSchema
} from '../domain/schemas/typebox/role.schema';
import {
  CreateUserSchema as TypeboxCreateUserSchema,
  UpdateUserSchema as TypeboxUpdateUserSchema,
  UserSchema as TypeboxUserSchema
} from '../domain/schemas/typebox/user.schema';
import {
  CreateOrganizationSchema as ZodCreateOrganizationSchema,
  OrganizationSchema as ZodOrganizationSchema,
  UpdateOrganizationSchema as ZodUpdateOrganizationSchema
} from '../domain/schemas/zod/organization.schema';
import {
  CreatePermissionSchema as ZodCreatePermissionSchema,
  PermissionSchema as ZodPermissionSchema,
  UpdatePermissionSchema as ZodUpdatePermissionSchema
} from '../domain/schemas/zod/permission.schema';
import {
  CreateRoleSchema as ZodCreateRoleSchema,
  RoleSchema as ZodRoleSchema,
  UpdateRoleSchema as ZodUpdateRoleSchema
} from '../domain/schemas/zod/role.schema';
import {
  CreateUserSchema as ZodCreateUserSchema,
  UpdateUserSchema as ZodUpdateUserSchema,
  UserSchema as ZodUserSchema
} from '../domain/schemas/zod/user.schema';

const zodUpdatePermissionSchema = ZodUpdatePermissionSchema({
  uuidId: false
});
const typeboxUpdatePermissionSchema = TypeboxUpdatePermissionSchema({
  uuidId: false
});
const zodPermissionSchema = ZodPermissionSchema({ uuidId: false });
const typeboxPermissionSchema = TypeboxPermissionSchema({ uuidId: false });

const zodUpdateRoleSchema = ZodUpdateRoleSchema({ uuidId: false });
const typeboxUpdateRoleSchema = TypeboxUpdateRoleSchema({ uuidId: false });
const zodRoleSchema = ZodRoleSchema({ uuidId: false });
const typeboxRoleSchema = TypeboxRoleSchema({ uuidId: false });

const zodUpdateUserSchema = ZodUpdateUserSchema({ uuidId: false });
const typeboxUpdateUserSchema = TypeboxUpdateUserSchema({ uuidId: false });
const zodUserSchema = ZodUserSchema({ uuidId: false });
const typeboxUserSchema = TypeboxUserSchema({ uuidId: false });

const zodUpdateOrganizationSchema = ZodUpdateOrganizationSchema({
  uuidId: false
});
const typeboxUpdateOrganizationSchema = TypeboxUpdateOrganizationSchema({
  uuidId: false
});
const zodOrganizationSchema = ZodOrganizationSchema({ uuidId: false })(
  DummyEnum
);
const typeboxOrganizationSchema = TypeboxOrganizationSchema({ uuidId: false })(
  DummyEnum
);

describe('schema equality', () => {
  it('should be equal for permission', () => {
    expect(
      isTrue(
        testSchemaEquality<CreatePermissionDto>()(
          ZodCreatePermissionSchema,
          TypeboxCreatePermissionSchema,
          {
            slug: 'test',
            addToRolesIds: ['test'],
            providerFields: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<UpdatePermissionDto>()(
          zodUpdatePermissionSchema,
          typeboxUpdatePermissionSchema,
          {
            id: 'test',
            slug: 'test',
            addToRolesIds: ['test'],
            removeFromRolesIds: ['test'],
            providerFields: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<PermissionDto>()(
          zodPermissionSchema,
          typeboxPermissionSchema,
          {
            id: 'test',
            slug: 'test',
            providerFields: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();
  });

  it('should be equal for role', () => {
    expect(
      isTrue(
        testSchemaEquality<CreateRoleDto>()(
          ZodCreateRoleSchema,
          TypeboxCreateRoleSchema,
          {
            name: 'test',
            permissionIds: ['test'],
            providerFields: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<UpdateRoleDto>()(
          zodUpdateRoleSchema,
          typeboxUpdateRoleSchema,
          {
            id: 'test',
            name: 'test',
            permissionIds: ['test'],
            providerFields: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<RoleDto>()(zodRoleSchema, typeboxRoleSchema, {
          id: 'test',
          name: 'test',
          permissions: [
            {
              id: 'test',
              slug: 'test',
              providerFields: {
                test: 'test'
              }
            }
          ],
          providerFields: {
            test: 'test'
          }
        })
      )
    ).toBeTruthy();
  });

  it('should be equal for user', () => {
    expect(
      isTrue(
        testSchemaEquality<CreateUserDto>()(
          ZodCreateUserSchema,
          TypeboxCreateUserSchema,
          {
            email: 'test@test.com',
            password: 'test',
            firstName: 'test',
            lastName: 'test',
            organization: 'test',
            roles: ['test'],
            phoneNumber: 'test',
            subscription: 'test',
            providerFields: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<UpdateUserDto>()(
          zodUpdateUserSchema,
          typeboxUpdateUserSchema,
          {
            id: 'test',
            email: 'test@test.com',
            password: 'test',
            firstName: 'test',
            lastName: 'test',
            roles: ['test'],
            phoneNumber: 'test',
            subscription: 'test',
            providerFields: {
              test: 'test'
            }
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<UserDto>()(zodUserSchema, typeboxUserSchema, {
          id: 'test',
          email: 'test@test.com',
          firstName: 'test',
          lastName: 'test',
          roles: [
            {
              id: 'test',
              name: 'test',
              permissions: [{ id: 'test', slug: 'test' }],
              providerFields: {
                test: 'test'
              }
            }
          ],
          phoneNumber: 'test',
          subscription: 'test',
          providerFields: {
            test: 'test'
          },
          organization: 'test'
        })
      )
    ).toBeTruthy();
  });

  it('should be equal for organization', () => {
    expect(
      isTrue(
        testSchemaEquality<CreateOrganizationDto>()(
          ZodCreateOrganizationSchema,
          TypeboxCreateOrganizationSchema,
          {
            name: 'test',
            domain: 'test',
            subscription: 'test',
            logoUrl: 'test',
            providerFields: { test: 'test' }
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<UpdateOrganizationDto>()(
          zodUpdateOrganizationSchema,
          typeboxUpdateOrganizationSchema,
          {
            id: 'test',
            name: 'test',
            domain: 'test',
            subscription: 'test',
            logoUrl: 'test',
            providerFields: { test: 'test' }
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<OrganizationDto<typeof DummyEnum>>()(
          zodOrganizationSchema,
          typeboxOrganizationSchema,
          {
            id: 'test',
            name: 'test',
            domain: 'test',
            subscription: 'test',
            logoUrl: 'test',
            providerFields: { test: 'test' },
            status: DummyEnum.A,
            users: [
              {
                id: 'test',
                email: 'test@test.com',
                firstName: 'test',
                lastName: 'test',
                roles: [
                  {
                    id: 'test',
                    name: 'test',
                    permissions: [{ id: 'test', slug: 'test' }],
                    providerFields: { test: 'test' }
                  }
                ],
                organization: 'test'
              }
            ]
          }
        )
      )
    ).toBeTruthy();
  });
});

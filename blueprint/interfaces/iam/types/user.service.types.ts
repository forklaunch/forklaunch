import { IdDto, IdsDto, RecordTimingDto } from '@forklaunch/common';
import { RoleDto } from './role.service.types';

export type CreateUserDto = Partial<IdDto> & {
  email: string;
  password: string;
  firstName: string;
  lastName: string;
  organization?: string;
  roles: string[];
  phoneNumber?: string;
  subscription?: string;
  providerFields?: unknown;
};
export type UpdateUserDto = Partial<Omit<CreateUserDto, 'organization'>> &
  IdDto;

export type UserDto = Omit<CreateUserDto, 'roles' | 'password'> &
  IdDto &
  Partial<RecordTimingDto> & {
    roles: RoleDto[];
  };

export type UserServiceParameters = {
  CreateUserDto: CreateUserDto;
  UserDto: UserDto;
  UpdateUserDto: UpdateUserDto;
  IdDto: IdDto;
  IdsDto: IdsDto;
};

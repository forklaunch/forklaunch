import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

export const Organization = defineComplianceEntity({
  name: 'Organization',
  properties: {
    id: fp.string().primary().compliance('none'),
    status: fp.enum().compliance('none')
  }
});

export const Role = defineComplianceEntity({
  name: 'Role',
  properties: {
    id: fp.string().primary().compliance('none'),
    permissions: () => fp.manyToMany(Permission)
  }
});

export const Permission = defineComplianceEntity({
  name: 'Permission',
  properties: {
    id: fp.string().primary().compliance('none'),
    slug: fp.string().compliance('none')
  }
});

export const User = defineComplianceEntity({
  name: 'User',
  properties: {
    id: fp.string().primary().compliance('none'),
    organization: () => fp.manyToOne(Organization).nullable()
  }
});

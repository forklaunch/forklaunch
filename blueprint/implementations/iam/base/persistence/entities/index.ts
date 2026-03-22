import { defineEntity, p } from '@mikro-orm/core';

export const Organization = defineEntity({
  name: 'Organization',
  properties: {
    id: p.string().primary(),
    status: p.enum()
  }
});

export const Role = defineEntity({
  name: 'Role',
  properties: {
    id: p.string().primary(),
    permissions: () => p.manyToMany(Permission)
  }
});

export const Permission = defineEntity({
  name: 'Permission',
  properties: {
    id: p.string().primary(),
    slug: p.string()
  }
});

export const User = defineEntity({
  name: 'User',
  properties: {
    id: p.string().primary(),
    organization: () => p.manyToOne(Organization).nullable()
  }
});

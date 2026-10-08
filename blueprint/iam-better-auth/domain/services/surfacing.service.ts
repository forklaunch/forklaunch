import { EntityManager } from '@mikro-orm/postgresql';
import { Member } from '../../persistence/entities/member.entity';
import { OrganizationRole } from '../../persistence/entities/organizationRole.entity';

export class SurfacingService {
  constructor(private readonly em: EntityManager) {}

  async surfaceRole(
    userId: string,
    organizationId: string
  ): Promise<string | null> {
    if (!userId || !organizationId) return null;

    const member = await this.em.findOne(Member, {
      userId,
      organizationId
    });
    return member?.role ?? null;
  }

  async surfacePermissions(
    userId: string,
    organizationId: string
  ): Promise<string[]> {
    if (!userId || !organizationId) return [];

    const role = await this.surfaceRole(userId, organizationId);
    if (!role) return [];

    const orgRoles = await this.em.find(OrganizationRole, {
      organizationId,
      role
    });
    return orgRoles.map((r) => r.permission);
  }
}

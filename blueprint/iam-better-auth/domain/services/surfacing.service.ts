import { EntityManager } from '@mikro-orm/postgresql';
import { Member } from '../../persistence/entities/member.entity';
import { OrganizationRole } from '../../persistence/entities/organizationRole.entity';
import { Session } from '../../persistence/entities/session.entity';

export class SurfacingService {
  constructor(private readonly em: EntityManager) {}

  private async getActiveOrganizationId(
    userId: string
  ): Promise<string | null> {
    const session = await this.em.findOne(
      Session,
      {
        user: userId,
        activeOrganizationId: { $ne: null },
        expiresAt: { $gt: new Date() }
      },
      { orderBy: { createdAt: 'DESC' } }
    );
    return session?.activeOrganizationId ?? null;
  }

  async surfaceRole(userId: string): Promise<string | null> {
    const activeOrganizationId = await this.getActiveOrganizationId(userId);
    if (!activeOrganizationId) return null;

    const member = await this.em.findOne(Member, {
      userId,
      organizationId: activeOrganizationId
    });
    return member?.role ?? null;
  }

  async surfacePermissions(userId: string): Promise<string[]> {
    const activeOrganizationId = await this.getActiveOrganizationId(userId);
    if (!activeOrganizationId) return [];

    const role = await this.surfaceRole(userId);
    if (!role) return [];

    const orgRoles = await this.em.find(OrganizationRole, {
      organizationId: activeOrganizationId,
      role
    });
    return orgRoles.map((r) => r.permission);
  }
}

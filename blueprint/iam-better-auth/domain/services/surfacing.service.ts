import type { EntityManager } from '@mikro-orm/postgresql';
import { Member } from '../../persistence/entities/member.entity';
import { OrganizationRole } from '../../persistence/entities/organizationRole.entity';
import { Session } from '../../persistence/entities/session.entity';
import { builtinOrganizationPermissions } from '../utils/organizationRoleStatements.util';
import {
  getAuthorizationScope,
  type AuthorizationScope
} from '../utils/authorizationScope.util';

export class SurfacingService {
  constructor(private readonly em: EntityManager) {}

  private async currentMember(
    scope: AuthorizationScope
  ): Promise<Member | null> {
    if (
      !getAuthorizationScope({
        sub: scope?.userId,
        activeOrganizationId: scope?.organizationId,
        sessionId: scope?.sessionId
      })
    )
      return null;
    // Never choose another session for this user, even if it is newer.
    const session = await this.em.findOne(
      Session,
      {
        id: scope.sessionId,
        user: scope.userId,
        activeOrganizationId: scope.organizationId,
        expiresAt: { $gt: new Date() }
      },
      { refresh: true }
    );
    if (!session) return null;
    return this.em.findOne(
      Member,
      {
        userId: scope.userId,
        organizationId: scope.organizationId
      },
      { refresh: true }
    );
  }

  async surfaceRole(scope: AuthorizationScope): Promise<string | null> {
    return (await this.currentMember(scope))?.role ?? null;
  }

  async surfacePermissions(scope: AuthorizationScope): Promise<string[]> {
    const member = await this.currentMember(scope);
    if (!member) return [];
    const builtin = builtinOrganizationPermissions(member.role);
    if (builtin.length) return builtin;
    const orgRoles = await this.em.find(
      OrganizationRole,
      {
        organizationId: scope.organizationId,
        role: member.role
      },
      { refresh: true }
    );
    return orgRoles.map((row) => row.permission);
  }
}

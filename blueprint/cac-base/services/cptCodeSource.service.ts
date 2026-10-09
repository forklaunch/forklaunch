import { CptCodeSource } from '@forklaunch/implementation-cac-base/services';
import { EntityManager } from '@mikro-orm/postgresql';
import { CptCode } from '../persistence/entities/cptCode.entity';

// The concrete, ready-to-use CptCodeSource: looks codes up against the
// CptCode reference table, which stays empty until an organization runs
// scripts/refresh-code-sets.ts against their own licensed CPT feed (§5).
// This is the "here's a real, working starting point" half of the readiness
// bar — CptCodeProvider (implementations/cac/base) doesn't care which
// CptCodeSource it's given, but this is the one cac-base wires up by
// default.
export class EntityManagerCptCodeSource implements CptCodeSource {
  constructor(
    private readonly em: EntityManager,
    private readonly organizationId: string
  ) {}

  async lookup(code: string) {
    const found = await this.em.findOne(CptCode, {
      code,
      organizationId: this.organizationId
    });
    return found ? { code, description: found.description } : undefined;
  }
}

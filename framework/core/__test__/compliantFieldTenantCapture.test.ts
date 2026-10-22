import { MikroORM, wrap, type EntityManager } from '@mikro-orm/libsql';
import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  FieldEncryptor,
  TenantMismatchError,
  UnboundTenantError,
  defineComplianceEntity,
  deanon,
  fp,
  onComplianceAccess,
  registerEncryptor,
  setupTenantFilter,
  withEncryptionContext,
  wrapEmWithTenantContext,
  type ComplianceAccessEvent
} from '../src/persistence';

/**
 * A COMPLIANT FIELD DECRYPTS UNDER THE TENANT IT WAS LOADED UNDER.
 *
 * `.deanon` decrypts lazily. It used to decrypt under whatever tenant was
 * bound at read time, which broke the two things applications do right after
 * a query returns:
 *
 * - build a DTO outside the tenant-bound entity manager: nothing is bound
 *   there, so every read threw UnboundTenantError;
 * - wrap that DTO building in `withEncryptionContext(tenantFromRequest, …)`:
 *   the read then used a tenant chosen by the request, not the one the row
 *   was loaded under.
 *
 * The field now records the tenant bound when it was hydrated (or when its
 * plaintext was assigned) and always uses that one. A different bound tenant
 * is refused.
 */

const Member = defineComplianceEntity({
  name: 'TenantCaptureMember',
  properties: {
    id: fp.integer().primary().autoincrement().compliance('none'),
    organizationId: fp.string().compliance('none'),
    name: fp.string().compliance('phi'),
    email: fp.string().compliance('pii', { queryable: true }),
    week: fp.integer().compliance('phi')
  }
});

const A = 'org-a';
const B = 'org-b';

let orm: MikroORM;
const accessLog: ComplianceAccessEvent[] = [];

/** What a DI factory hands a request: a fork bound to the request's tenant. */
const requestEm = (tenantId: string): EntityManager =>
  wrapEmWithTenantContext(orm.em.fork(), tenantId) as EntityManager;

beforeAll(async () => {
  registerEncryptor(new FieldEncryptor('tenant-capture-test-master-key'));
  onComplianceAccess((event) => accessLog.push(event));
  orm = await MikroORM.init({
    entities: [Member],
    dbName: ':memory:',
    allowGlobalContext: true
  });
  setupTenantFilter(orm, { logger: { info: () => undefined } });
  await orm.schema.create();
});

beforeEach(async () => {
  accessLog.length = 0;
  await orm.em.getConnection().execute('delete from tenant_capture_member');
});

afterAll(async () => {
  await orm?.close();
});

async function seed(
  tenantId: string,
  name: string,
  email = `${name.toLowerCase()}@example.com`
) {
  const em = requestEm(tenantId);
  const member = em.create(Member, {
    organizationId: tenantId,
    name,
    email,
    week: 12
  });
  await em.flush();
  return member.id;
}

describe('compliant fields keep the tenant they were loaded under', () => {
  it('decrypts with no tenant bound after the loading call has returned', async () => {
    const id = await seed(A, 'Ada');
    const member = await requestEm(A).findOneOrFail(Member, id);
    // Outside the proxy: nothing is bound here.
    expect(member.name.deanon).toBe('Ada');
    expect(member.week.deanon).toBe(12);
    // A queryable field is built on first access, after hydration.
    expect(member.email.deanon).toBe('ada@example.com');
    expect(deanon(member)).toMatchObject({ name: 'Ada', week: 12 });
  });

  it('refuses to decrypt inside another tenant', async () => {
    const id = await seed(A, 'Ada');
    const member = await requestEm(A).findOneOrFail(Member, id);
    expect(() => withEncryptionContext(B, () => member.name.deanon)).toThrow(
      TenantMismatchError
    );
    expect(() => withEncryptionContext(B, () => member.email.deanon)).toThrow(
      TenantMismatchError
    );
    expect(accessLog).toEqual([]);
  });

  it('refuses another tenant even once the plaintext is in memory', async () => {
    const id = await seed(A, 'Ada');
    const member = await requestEm(A).findOneOrFail(Member, id);
    expect(member.name.deanon).toBe('Ada');
    expect(() => withEncryptionContext(B, () => member.name.deanon)).toThrow(
      TenantMismatchError
    );
  });

  it('still decrypts inside the owning tenant', async () => {
    const id = await seed(A, 'Ada');
    const member = await requestEm(A).findOneOrFail(Member, id);
    expect(withEncryptionContext(A, () => member.name.deanon)).toBe('Ada');
  });

  it("does not let a request's tenant decrypt another tenant's row", async () => {
    // The isolation case: member B's request has a row of tenant A in hand
    // (a superadmin read, a shared cache) and binds its own tenant to build
    // the response. The row decrypts under A or not at all.
    const id = await seed(A, 'Ada');
    const row = await getSuperAdminRow(id);
    expect(() => withEncryptionContext(B, () => row.name.deanon)).toThrow(
      TenantMismatchError
    );
  });

  it("decrypts two tenants' rows loaded in one request with each one's key", async () => {
    const idA = await seed(A, 'Ada');
    const idB = await seed(B, 'Bea');
    // One unfiltered manager, two loads under two bound tenants.
    const em = orm.em.fork();
    const fromA = await withEncryptionContext(A, () =>
      em.findOneOrFail(Member, idA)
    );
    const fromB = await withEncryptionContext(B, () =>
      em.findOneOrFail(Member, idB)
    );
    expect(fromA.name.deanon).toBe('Ada');
    expect(fromB.name.deanon).toBe('Bea');
    expect(fromA.email.deanon).toBe('ada@example.com');
    expect(fromB.email.deanon).toBe('bea@example.com');
    expect(accessLog.map((e) => [e.field, e.tenantId])).toEqual([
      ['TenantCaptureMember.name', A],
      ['TenantCaptureMember.name', B],
      ['TenantCaptureMember.email', A],
      ['TenantCaptureMember.email', B]
    ]);
  });

  it('reports the owning tenant to access listeners', async () => {
    const id = await seed(A, 'Ada');
    const member = await requestEm(A).findOneOrFail(Member, id);
    member.name.deanon;
    expect(accessLog).toEqual([
      { field: 'TenantCaptureMember.name', level: 'phi', tenantId: A }
    ]);
  });

  it('needs a bound tenant to read a row loaded with none bound', async () => {
    const id = await seed(A, 'Ada');
    const member = await orm.em.fork().findOneOrFail(Member, id);
    expect(() => member.name.deanon).toThrow(UnboundTenantError);
    expect(withEncryptionContext(A, () => member.name.deanon)).toBe('Ada');
  });

  it('reads a legacy queryable row (no index yet) under its loading tenant', async () => {
    const id = await seed(A, 'Ada');
    await orm.em
      .getConnection()
      .execute(
        `update tenant_capture_member set email_idx = null where id = ?`,
        [id]
      );
    const member = await requestEm(A).findOneOrFail(Member, id);
    expect(member.email.deanon).toBe('ada@example.com');
  });
});

describe('writes stay with the tenant a value belongs to', () => {
  it('seals a value assigned outside the proxy under the flushing tenant', async () => {
    const id = await seed(A, 'Ada');
    const em = requestEm(A);
    const member = await em.findOneOrFail(Member, id);
    em.assign(member, { name: 'Ada L.' });
    await em.flush();
    const reloaded = await requestEm(A).findOneOrFail(Member, id);
    expect(reloaded.name.deanon).toBe('Ada L.');
  });

  it("refuses to flush a value assigned under one tenant into another's", async () => {
    const id = await seed(B, 'Bea');
    const em = requestEm(B);
    const member = await em.findOneOrFail(Member, id);
    // Not em.assign: the proxy would run it under B. Code holding a tenant-A
    // context assigns directly.
    withEncryptionContext(A, () => wrap(member).assign({ name: 'Moved' }));
    await expect(em.flush()).rejects.toThrow(TenantMismatchError);
  });

  it('keeps a value with the tenant that first sealed it', async () => {
    // Assigned with no tenant bound, flushed under A: it is A's from then on.
    // A later flush of the same manager under B neither re-seals it under
    // B's key nor fails for a row it does not touch.
    const em = orm.em.fork();
    const member = em.create(Member, {
      organizationId: A,
      name: 'Ada',
      email: 'ada@example.com',
      week: 1
    });
    await withEncryptionContext(A, () => em.flush());
    const [before] = await orm.em
      .getConnection()
      .execute(`select name from tenant_capture_member where id = ?`, [
        member.id
      ]);
    await withEncryptionContext(B, () => em.flush());
    const [after] = await orm.em
      .getConnection()
      .execute(`select name from tenant_capture_member where id = ?`, [
        member.id
      ]);
    expect(after.name).toBe(before.name);
    expect(member.name.deanon).toBe('Ada');
    expect(() => withEncryptionContext(B, () => member.name.deanon)).toThrow(
      TenantMismatchError
    );
  });

  it('still refuses to write with no tenant at all', async () => {
    const em = orm.em.fork();
    em.create(Member, {
      organizationId: A,
      name: 'Nobody',
      email: 'nobody@example.com',
      week: 1
    });
    await expect(em.flush()).rejects.toThrow(UnboundTenantError);
  });
});

async function getSuperAdminRow(id: number) {
  return withEncryptionContext(A, () =>
    orm.em.fork().findOneOrFail(Member, id)
  );
}

describe('em.assign on encrypted scalars of an existing row', () => {
  const Profile = defineComplianceEntity({
    name: 'AssignProfile',
    properties: {
      id: fp.integer().primary().autoincrement().compliance('none'),
      week: fp.integer().compliance('phi'),
      weight: fp.double().compliance('phi'),
      pregnant: fp.boolean().compliance('phi'),
      dueDate: fp.datetime().compliance('phi'),
      vitals: fp.json<{ bp: string; hr: number }>().compliance('phi'),
      note: fp.string().compliance('phi'),
      mrn: fp.string().compliance('phi', { queryable: true }),
      tags: fp.array().compliance('phi')
    }
  });

  let profileOrm: MikroORM;

  beforeAll(async () => {
    profileOrm = await MikroORM.init({
      entities: [Profile],
      dbName: ':memory:',
      allowGlobalContext: true
    });
    await profileOrm.schema.create();
  });

  afterAll(async () => {
    await profileOrm?.close();
  });

  beforeEach(async () => {
    await profileOrm.em.getConnection().execute('delete from assign_profile');
  });

  const em = () =>
    wrapEmWithTenantContext(profileOrm.em.fork(), A) as EntityManager;

  async function seedProfile() {
    const e = em();
    const profile = e.create(Profile, {
      week: 28,
      weight: 61.5,
      pregnant: false,
      dueDate: new Date('2027-01-01T00:00:00Z'),
      vitals: { bp: '110/70', hr: 72 },
      note: 'first',
      mrn: 'MRN-1',
      tags: ['a']
    });
    await e.flush();
    return profile.id;
  }

  const changes = {
    week: 29,
    weight: 62.25,
    pregnant: true,
    dueDate: new Date('2027-02-03T00:00:00Z'),
    vitals: { bp: '120/80', hr: 80 },
    note: 'second',
    mrn: 'MRN-2',
    tags: ['b', 'c']
  };

  async function expectChanged(id: number) {
    const profile = await em().findOneOrFail(Profile, id);
    expect(profile.week.deanon).toBe(29);
    expect(profile.weight.deanon).toBe(62.25);
    expect(profile.pregnant.deanon).toBe(true);
    expect(profile.dueDate.deanon).toEqual(changes.dueDate);
    expect(profile.vitals.deanon).toEqual(changes.vitals);
    expect(profile.note.deanon).toBe('second');
    expect(profile.mrn.deanon).toBe('MRN-2');
    expect(profile.tags.deanon).toEqual(['b', 'c']);
    expect(
      (await em().find(Profile, { mrn: 'MRN-2' })).map((p) => p.id)
    ).toEqual([id]);
  }

  it('em.assign', async () => {
    const id = await seedProfile();
    const e = em();
    const profile = await e.findOneOrFail(Profile, id);
    e.assign(profile, changes);
    await e.flush();
    await expectChanged(id);
  });

  it('wrap(entity).assign', async () => {
    const id = await seedProfile();
    const e = em();
    const profile = await e.findOneOrFail(Profile, id);
    wrap(profile).assign(changes);
    await e.flush();
    await expectChanged(id);
  });

  it.each(Object.keys(changes) as (keyof typeof changes)[])(
    'em.assign of %s alone',
    async (key) => {
      const id = await seedProfile();
      const e = em();
      const profile = await e.findOneOrFail(Profile, id);
      e.assign(profile, { [key]: changes[key] });
      await e.flush();
      const reloaded = await em().findOneOrFail(Profile, id);
      expect(reloaded[key].deanon).toEqual(changes[key]);
    }
  );
});

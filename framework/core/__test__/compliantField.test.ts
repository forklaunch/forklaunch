import { MikroORM, wrap } from '@mikro-orm/sqlite';
import { inspect } from 'node:util';
import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  Anon,
  CompliantQueryError,
  FieldEncryptor,
  LegacyCiphertextError,
  reencryptEncryptedColumns,
  defineComplianceEntity,
  fp,
  isAnon,
  isCompliantField,
  onComplianceAccess,
  registerEncryptor,
  withEncryptionContext,
  type ComplianceAccessEvent
} from '../src/persistence';

/**
 * COMPLIANT FIELDS, END TO END.
 *
 * A pii/phi/pci property loads as a CompliantField: `.anon` for the
 * de-identified value, `.deanon` for the plaintext, and nothing else. Plain
 * values can still be assigned (directly, em.create, em.assign) and queried
 * (`where`, for queryable fields); they are wrapped, sealed and indexed on
 * the way through.
 */

const Patient = defineComplianceEntity({
  name: 'Patient',
  properties: {
    id: fp.integer().primary().autoincrement().compliance('none'),
    name: fp.string().compliance('phi'),
    email: fp
      .string()
      .unique()
      .compliance('pii', { queryable: true, normalize: 'lowercase' }),
    mrn: fp.string().nullable().compliance('phi', { queryable: true }),
    dob: fp.datetime().compliance('phi'),
    zip: fp.string().compliance('pii', { anon: 'zip3' }),
    visits: fp.integer().compliance('phi'),
    note: fp.string().compliance('none')
  }
});

const TENANT = 'org-a';
const OTHER_TENANT = 'org-b';

let orm: MikroORM;
const sql: string[] = [];
const accessLog: ComplianceAccessEvent[] = [];

const inTenant = <T>(tenantId: string, fn: () => Promise<T>) =>
  withEncryptionContext(tenantId, fn);
const writes = (from: number) =>
  sql.slice(from).filter((q) => /^(insert|update)/i.test(q));

beforeAll(async () => {
  registerEncryptor(new FieldEncryptor('compliant-field-test-master-key'));
  onComplianceAccess((event) => accessLog.push(event));
  orm = await MikroORM.init({
    entities: [Patient],
    dbName: ':memory:',
    debug: ['query'],
    logger: (message) =>
      sql.push(
        message
          .replace(/\x1b\[[0-9;]*m/g, '')
          .replace(/^\[query\]\s*/, '')
          .replace(/\s*\[took.*$/, '')
      ),
    allowGlobalContext: true
  });
  await orm.schema.create();
});

beforeEach(async () => {
  await orm.em.getConnection().execute('delete from patient');
});

afterAll(async () => {
  await orm?.close();
});

async function seed() {
  return inTenant(TENANT, async () => {
    const em = orm.em.fork();
    const patient = em.create(Patient, {
      name: 'Jane Doe',
      email: 'Jane@Example.com',
      mrn: 'MRN-001',
      dob: new Date('1984-03-12T00:00:00Z'),
      zip: '94110',
      visits: 3,
      note: 'first visit'
    });
    await em.flush();
    return patient.id;
  });
}

describe('compliant fields', () => {
  it('wraps plain values given to em.create', async () => {
    await inTenant(TENANT, async () => {
      const em = orm.em.fork();
      const patient = em.create(Patient, {
        name: 'Ada Lovelace',
        email: 'ada@example.com',
        mrn: null,
        dob: new Date('1815-12-10T00:00:00Z'),
        zip: '03601',
        visits: 1,
        note: 'x'
      });
      expect(isCompliantField(patient.name)).toBe(true);
      expect(patient.name.anon.value).toMatch(/^tok_/);
      expect(patient.zip.anon.value).toBe('000'); // restricted Safe Harbor prefix
      await em.flush();
    });
  });

  it('stores only envelopes and blind indexes', async () => {
    const id = await seed();
    const [row] = await orm.em
      .getConnection()
      .execute(`select * from patient where id = ?`, [id]);
    expect(row.name).toMatch(/^v4:/);
    expect(row.email).toMatch(/^v4:/);
    expect(row.email_idx).toMatch(/^bi1:/);
    expect(row.mrn_idx).toMatch(/^bi1:/);
    expect(row.dob).toMatch(/^v4:/);
    expect(row.visits).toMatch(/^v4:/);
    expect(JSON.stringify(row)).not.toContain('Jane');
    expect(JSON.stringify(row)).not.toContain('MRN-001');
    expect(row.note).toBe('first visit');
  });

  it('reads values only through .anon and .deanon', async () => {
    const id = await seed();
    await inTenant(TENANT, async () => {
      const patient = await orm.em.fork().findOneOrFail(Patient, id);
      expect(patient.name.anon).toBeInstanceOf(Anon);
      expect(isAnon(patient.name.anon)).toBe(true);
      expect(patient.name.anon.value).toMatch(/^tok_/);
      expect(patient.dob.anon.value).toBe('1984');
      expect(patient.zip.anon.value).toBe('941');
      expect(patient.visits.anon.value).toBe('[redacted]');
      expect(patient.name.deanon).toBe('Jane Doe');
      expect(patient.dob.deanon).toEqual(new Date('1984-03-12T00:00:00Z'));
      expect(patient.visits.deanon).toBe(3);
      expect(patient.email.deanon).toBe('Jane@Example.com');
    });
  });

  it('keeps the anon token stable across loads', async () => {
    const id = await seed();
    const first = await inTenant(
      TENANT,
      async () =>
        (await orm.em.fork().findOneOrFail(Patient, id)).name.anon.value
    );
    const second = await inTenant(
      TENANT,
      async () =>
        (await orm.em.fork().findOneOrFail(Patient, id)).name.anon.value
    );
    expect(first).toBe(second);
  });

  it('leaks nothing through implicit conversions', async () => {
    const id = await seed();
    await inTenant(TENANT, async () => {
      const patient = await orm.em.fork().findOneOrFail(Patient, id);
      expect(`${patient.name}`).toBe('[object Object]');
      const json = JSON.stringify(patient);
      const pojo = JSON.stringify(wrap(patient).toObject());
      const inspected = inspect(patient, { depth: 3 });
      for (const out of [json, pojo, inspected]) {
        expect(out).not.toContain('Jane');
        expect(out).not.toContain('MRN-001');
        expect(out).not.toMatch(/v4:/);
      }
      expect(JSON.parse(json).name).toEqual({});
    });
  });

  it('does not rewrite anything on a no-op flush', async () => {
    const id = await seed();
    await inTenant(TENANT, async () => {
      const em = orm.em.fork();
      const patient = await em.findOneOrFail(Patient, id);
      patient.name.deanon; // decrypting must not dirty the entity
      const before = sql.length;
      await em.flush();
      expect(writes(before)).toEqual([]);
      patient.note = 'second visit';
      await em.flush();
      expect(writes(before)).toHaveLength(1);
      expect(writes(before)[0]).toMatch(/set `note` = \?/);
    });
  });

  it('wraps plain values given to em.assign and direct assignment', async () => {
    const id = await seed();
    await inTenant(TENANT, async () => {
      const em = orm.em.fork();
      const patient = await em.findOneOrFail(Patient, id);
      em.assign(patient, { name: 'Janet Doe', email: 'janet@example.com' });
      // Plain values type-check through em.create / em.assign; a direct
      // assignment works at runtime but the property's type is the field.
      // @ts-expect-error -- plaintext is not a CompliantField
      patient.mrn = 'MRN-002';
      expect(isCompliantField(patient.name)).toBe(true);
      expect(isCompliantField(patient.mrn)).toBe(true);
      await em.flush();
    });
    await inTenant(TENANT, async () => {
      const patient = await orm.em.fork().findOneOrFail(Patient, id);
      expect(patient.name.deanon).toBe('Janet Doe');
      expect(patient.email.deanon).toBe('janet@example.com');
      expect(patient.mrn?.deanon).toBe('MRN-002');
    });
  });

  it('answers equality queries on queryable fields with plain values', async () => {
    const id = await seed();
    await inTenant(TENANT, async () => {
      const em = orm.em.fork();
      const before = sql.length;
      const byEmail = await em.find(Patient, {
        email: '  JANE@example.com '
      });
      expect(sql.slice(before).join('\n')).toMatch(/`email_idx` = \?/);
      expect(byEmail.map((p) => p.id)).toEqual([id]);
      expect(
        (await em.find(Patient, { mrn: { $in: ['nope', 'MRN-001'] } })).map(
          (p) => p.id
        )
      ).toEqual([id]);
      // `exact` normalization: case matters for the MRN
      expect(await em.find(Patient, { mrn: 'mrn-001' })).toHaveLength(0);
      expect(await em.count(Patient, { mrn: { $ne: 'MRN-001' } })).toBe(0);
    });
  });

  it('refuses queries that would need the plaintext', async () => {
    await inTenant(TENANT, async () => {
      const em = orm.em.fork();
      await expect(
        em.find(Patient, { email: { $like: '%example%' } })
      ).rejects.toThrow(CompliantQueryError);
      // A non-queryable field does not accept a plain value in `where`, in
      // the types or at runtime.
      // @ts-expect-error -- `name` is not queryable
      await expect(em.find(Patient, { name: 'Jane Doe' })).rejects.toThrow(
        /cannot be queried/
      );
    });
  });

  it('enforces uniqueness on the blind index', async () => {
    await seed();
    await expect(seed()).rejects.toThrow(/UNIQUE/i);
  });

  it('isolates tenants: indexes and ciphertexts do not cross', async () => {
    const id = await seed();
    await inTenant(OTHER_TENANT, async () => {
      const em = orm.em.fork();
      expect(await em.find(Patient, { email: 'jane@example.com' })).toEqual([]);
      const patient = await em.findOneOrFail(Patient, id);
      expect(() => patient.name.deanon).toThrow();
    });
  });

  it('reports every .deanon to access listeners', async () => {
    const id = await seed();
    accessLog.length = 0;
    await inTenant(TENANT, async () => {
      const patient = await orm.em.fork().findOneOrFail(Patient, id);
      patient.name.anon; // anon is not an access
      patient.name.deanon;
      patient.email.deanon;
    });
    expect(accessLog).toEqual([
      { field: 'Patient.name', level: 'phi', tenantId: TENANT },
      { field: 'Patient.email', level: 'pii', tenantId: TENANT }
    ]);
  });

  it('clears a nullable queryable field, envelope and index together', async () => {
    const id = await seed();
    await inTenant(TENANT, async () => {
      const em = orm.em.fork();
      const patient = await em.findOneOrFail(Patient, id);
      patient.mrn = null;
      await em.flush();
    });
    const [row] = await orm.em
      .getConnection()
      .execute(`select mrn, mrn_idx from patient where id = ?`, [id]);
    expect(row).toEqual({ mrn: null, mrn_idx: null });
  });

  it('backfills a missing blind index on the next flush', async () => {
    const id = await seed();
    await orm.em
      .getConnection()
      .execute(`update patient set mrn_idx = null where id = ?`, [id]);
    await inTenant(TENANT, async () => {
      const em = orm.em.fork();
      const patient = await em.findOneOrFail(Patient, id);
      expect(patient.mrn?.deanon).toBe('MRN-001');
      await em.flush();
      expect(
        (await orm.em.fork().find(Patient, { mrn: 'MRN-001' })).map((p) => p.id)
      ).toContain(id);
    });
  });
});

describe('upgrading legacy rows', () => {
  it('seals v2 values into v4 and writes blind indexes, once', async () => {
    const encryptor = new FieldEncryptor('compliant-field-test-master-key');
    const legacy = (value: string) => encryptor.encrypt(value, TENANT);
    const connection = orm.em.getConnection();
    await connection.execute(
      `insert into patient (id, name, email, email_idx, mrn, mrn_idx, dob, zip, visits, note)
       values (900, ?, ?, null, ?, null, ?, ?, ?, 'legacy')`,
      [
        legacy('Old Name'),
        legacy('old@example.com'),
        legacy('MRN-900'),
        legacy('1970-01-02T00:00:00.000Z'),
        legacy('10001'),
        legacy('7')
      ]
    );

    // Before the sweep: readable, but no token and not findable.
    await inTenant(TENANT, async () => {
      const patient = await orm.em.fork().findOneOrFail(Patient, 900);
      expect(patient.name.deanon).toBe('Old Name');
      expect(patient.dob.anon.value).toBe('1970'); // derivable without a token
      expect(() => patient.name.anon).toThrow(LegacyCiphertextError);
      expect(
        await orm.em.fork().find(Patient, { email: 'old@example.com' })
      ).toEqual([]);
    });

    // The sweep's SQL is written for Postgres; answer its table probe here.
    const execute = (query: string, params?: unknown[]) =>
      /to_regclass/.test(query)
        ? Promise.resolve([{ t: 'patient' }])
        : connection.execute(query, params as never);
    const sweep = () =>
      reencryptEncryptedColumns({
        em: orm.em.fork() as never,
        encryptor,
        execute,
        tenantIdsFor: () => [TENANT]
      });

    const [first] = await sweep();
    expect(first.sealed).toBe(6);
    expect(first.reindexed).toBe(2);

    const [row] = await connection.execute(
      `select * from patient where id = 900`
    );
    for (const column of ['name', 'email', 'mrn', 'dob', 'zip', 'visits']) {
      expect(row[column]).toMatch(/^v4:/);
    }
    expect(row.email_idx).toMatch(/^bi1:/);

    await inTenant(TENANT, async () => {
      const patient = await orm.em.fork().findOneOrFail(Patient, 900);
      expect(patient.name.anon.value).toMatch(/^tok_/);
      expect(patient.zip.anon.value).toBe('100');
      expect(patient.visits.deanon).toBe(7);
      expect(
        (await orm.em.fork().find(Patient, { email: 'OLD@example.com' })).map(
          (p) => p.id
        )
      ).toEqual([900]);
    });

    // Idempotent: a second pass changes nothing.
    const [second] = await sweep();
    expect(second.sealed ?? 0).toBe(0);
    expect(second.reindexed ?? 0).toBe(0);
    expect(second.rewritten).toBe(0);
  });
});

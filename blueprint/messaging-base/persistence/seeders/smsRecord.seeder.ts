import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { SmsRecord } from '../entities/smsRecord.entity';
import { smsRecord } from '../seed.data';

export class SmsRecordSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdSmsRecord = em.create(SmsRecord, smsRecord);
    await em.persist(createdSmsRecord).flush();
  }
}

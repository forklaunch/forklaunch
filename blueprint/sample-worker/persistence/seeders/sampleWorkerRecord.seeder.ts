import { EntityManager } from '@mikro-orm/core';
import { Seeder } from '@mikro-orm/seeder';
import { SampleWorkerEventRecord } from '../entities/sampleWorkerRecord.entity';
import { sampleWorkerEventRecord } from '../seed.data';

export class SampleWorkerEventRecordSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    const createdRecord = em.create(
      SampleWorkerEventRecord,
      sampleWorkerEventRecord
    );
    await em.persist(createdRecord).flush();
  }
}

import { EntityManager } from "@mikro-orm/core";
import { Seeder } from "@mikro-orm/seeder";
import { {{camel_case_name}}{{#is_worker}}Event{{/is_worker}}RecordData } from "../seed.data";
import { {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record } from "../entities/{{camel_case_name}}{{#is_worker}}Event{{/is_worker}}Record.entity";

export class {{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}RecordSeeder extends Seeder {
  async run(em: EntityManager): Promise<void> {
    em.setFilterParams('tenant', { tenantId: 'seed' });
    const record = em.create({{pascal_case_name}}{{#is_worker}}Event{{/is_worker}}Record, {{camel_case_name}}{{#is_worker}}Event{{/is_worker}}RecordData);
    await em.persist(record).flush();
  }
}

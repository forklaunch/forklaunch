import { Migration } from '@mikro-orm/migrations';

// Figures showing a procedure's steps, per framework item, found when the
// page is assembled.
export class Migration20261009000000 extends Migration {
  override name = 'Migration20261009000000';

  override up(): void | Promise<void> {
    this.addSql(`alter table "topic" add column "figures" jsonb null;`);
  }

  override down(): void | Promise<void> {
    this.addSql(`alter table "topic" drop column "figures";`);
  }

}

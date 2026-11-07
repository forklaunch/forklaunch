import { Migration } from '@mikro-orm/migrations';

export class Migration20261006000000 extends Migration {

  override name = 'Migration20261006000000';

  // generated_answer.query becomes encrypted per organization (pii), with
  // the organization and user it belongs to, so it can be anonymized after
  // 90 days, exported and erased like search history. Query text already
  // stored was written in plaintext; it is removed rather than left
  // readable, and the audit rows themselves are kept.
  override up(): void | Promise<void> {
    this.addSql(`alter table "generated_answer" add column "organization_id" text null, add column "user_id" text null;`);
    this.addSql(`update "generated_answer" set "query" = null where "query" is not null;`);
    this.addSql(`create index "generated_answer_user_index" on "generated_answer" ("user_id") where "user_id" is not null;`);
  }

  override down(): void | Promise<void> {
    this.addSql(`drop index if exists "generated_answer_user_index";`);
    this.addSql(`alter table "generated_answer" drop column "organization_id", drop column "user_id";`);
  }

}

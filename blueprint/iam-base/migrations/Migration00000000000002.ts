import { Migration } from '@mikro-orm/migrations';

export class Migration00000000000002 extends Migration {
  override name = 'Migration00000000000002';

  override up(): void | Promise<void> {
    this.addSql(
      `alter table "user" drop constraint "user_organization_id_foreign";`
    );

    this.addSql(
      `alter table "organization" add "retention_anonymized_at" timestamptz null, add "provider_fields" jsonb null;`
    );
    this.addSql(
      `alter table "organization" add constraint "organization_status_check" check ("status" in ('active', 'inactive'));`
    );

    this.addSql(
      `alter table "permission" add "retention_anonymized_at" timestamptz null;`
    );

    this.addSql(
      `alter table "role" add "retention_anonymized_at" timestamptz null;`
    );

    this.addSql(
      `alter table "user" add "retention_anonymized_at" timestamptz null;`
    );
    this.addSql(
      `alter table "user" add constraint "user_organization_id_foreign" foreign key ("organization_id") references "organization" ("id") on delete set null;`
    );
  }

  override down(): void | Promise<void> {
    this.addSql(
      `alter table "user" drop constraint "user_organization_id_foreign";`
    );

    this.addSql(
      `alter table "organization" drop constraint "organization_status_check";`
    );
    this.addSql(
      `alter table "organization" drop column "retention_anonymized_at", drop column "provider_fields";`
    );

    this.addSql(
      `alter table "permission" drop column "retention_anonymized_at";`
    );

    this.addSql(`alter table "role" drop column "retention_anonymized_at";`);

    this.addSql(`alter table "user" drop column "retention_anonymized_at";`);
    this.addSql(
      `alter table "user" add constraint "user_organization_id_foreign" foreign key ("organization_id") references "organization" ("id") on update cascade on delete set null;`
    );
  }
}

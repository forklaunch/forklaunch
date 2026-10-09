import { Migration } from '@mikro-orm/migrations';

export class Migration20260925000000 extends Migration {

  override name = 'Migration20260925000000';

  override up(): void | Promise<void> {
    // Vector search needs the pgvector extension. The CLI provisions a
    // pgvector-enabled PostgreSQL image for any application that includes
    // mlse; on plain PostgreSQL this statement fails, which is the intended
    // signal rather than a search feature that silently does nothing.
    this.addSql(`create extension if not exists vector;`);

    this.addSql(`create table "source" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "source_key" text not null, "name" text not null, "tier" text not null, "license_terms" text not null, "commercial_use" boolean not null, "live_query" boolean not null, "last_refreshed_at" timestamptz null, primary key ("id"));`);
    this.addSql(`alter table "source" add constraint "source_source_key_unique" unique ("source_key");`);
  }

  override down(): void | Promise<void> {
    this.addSql(`drop table if exists "source" cascade;`);
    // The extension is left in place: other schemas in the same database may
    // depend on it, and dropping it would break them.
  }

}

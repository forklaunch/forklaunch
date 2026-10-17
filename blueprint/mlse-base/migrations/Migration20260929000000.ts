import { Migration } from '@mikro-orm/migrations';

export class Migration20260929000000 extends Migration {

  override name = 'Migration20260929000000';

  override up(): void | Promise<void> {
    this.addSql(`alter table "source" add column "requires_license" boolean not null default false;`);

    this.addSql(`create table "voice_setting" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "organization_id" text not null, "area" text not null, "enabled" boolean not null default false, "updated_by" text not null, primary key ("id"));`);
    this.addSql(`create unique index "voice_setting_organization_area_unique" on "voice_setting" ("organization_id", "area");`);

    this.addSql(`create table "content_license" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "organization_id" text not null, "source_key" text not null, "licensee" text not null, "reference" text null, "status" text not null, "valid_from" timestamptz not null, "valid_until" timestamptz null, "created_by" text not null, primary key ("id"));`);
    this.addSql(`alter table "content_license" add constraint "content_license_status_check" check ("status" in ('active', 'revoked'));`);
    this.addSql(`create index "content_license_organization_source_index" on "content_license" ("organization_id", "source_key");`);

    this.addSql(`create table "content_flag" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "document_id" uuid not null, "reason" text not null, "status" text not null, "flagged_by" text not null, "resolved_by" text null, "resolution" text null, "resolved_at" timestamptz null, primary key ("id"));`);
    this.addSql(`alter table "content_flag" add constraint "content_flag_document_id_foreign" foreign key ("document_id") references "document" ("id") on update cascade on delete cascade;`);
    this.addSql(`alter table "content_flag" add constraint "content_flag_status_check" check ("status" in ('open', 'resolved', 'rejected'));`);
    this.addSql(`create index "content_flag_open_index" on "content_flag" ("document_id") where "status" = 'open';`);

    this.addSql(`create table "saved_search" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "organization_id" text not null, "user_id" text not null, "name" text not null, "query" text not null, "topic_slug" text null, primary key ("id"));`);
    this.addSql(`create index "saved_search_organization_user_index" on "saved_search" ("organization_id", "user_id");`);

    this.addSql(`create table "search_history" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "organization_id" text not null, "user_id" text not null, "query" text null, "query_class" text not null, "channel" text not null, "answer_id" text null, primary key ("id"));`);
    this.addSql(`create index "search_history_organization_user_index" on "search_history" ("organization_id", "user_id", "created_at");`);
  }

  override down(): void | Promise<void> {
    this.addSql(`drop table if exists "search_history" cascade;`);
    this.addSql(`drop table if exists "saved_search" cascade;`);
    this.addSql(`drop table if exists "content_flag" cascade;`);
    this.addSql(`drop table if exists "content_license" cascade;`);
    this.addSql(`drop table if exists "voice_setting" cascade;`);
    this.addSql(`alter table "source" drop column "requires_license";`);
  }

}

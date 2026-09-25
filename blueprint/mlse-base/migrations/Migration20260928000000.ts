import { Migration } from '@mikro-orm/migrations';

export class Migration20260928000000 extends Migration {

  override name = 'Migration20260928000000';

  override up(): void | Promise<void> {
    this.addSql(`create table "generated_answer" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "query" text null, "query_class" text not null, "classification_reason" text not null, "kind" text not null, "topic_slug" text null, "provider" text null, "model" text null, "sections" jsonb not null, "sentences_kept" int not null, "sentences_removed" int not null, "removed_sentences" jsonb not null, "duration_ms" int not null, primary key ("id"));`);
    this.addSql(`alter table "generated_answer" add constraint "generated_answer_kind_check" check ("kind" in ('answer', 'label_range', 'boundary', 'emergency', 'source_not_found'));`);
    this.addSql(`create index "generated_answer_created_at_index" on "generated_answer" ("created_at");`);

    this.addSql(`create table "answer_citation" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "answer_id" uuid not null, "passage_id" text not null, "origin" text not null, "source_key" text not null, "external_id" text not null, "title" text not null, "url" text not null, "section_path" text not null, "license_scope" text not null, primary key ("id"));`);
    this.addSql(`alter table "answer_citation" add constraint "answer_citation_answer_id_foreign" foreign key ("answer_id") references "generated_answer" ("id") on update cascade on delete cascade;`);
    this.addSql(`create index "answer_citation_answer_id_index" on "answer_citation" ("answer_id");`);
  }

  override down(): void | Promise<void> {
    this.addSql(`drop table if exists "answer_citation" cascade;`);
    this.addSql(`drop table if exists "generated_answer" cascade;`);
  }

}

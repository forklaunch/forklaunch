import { Migration } from '@mikro-orm/migrations';

export class Migration20260927000000 extends Migration {

  override name = 'Migration20260927000000';

  override up(): void | Promise<void> {
    this.addSql(`create table "topic" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "slug" text not null, "topic_type" text not null, "title" text not null, "framework_key" text not null, "mesh_descriptor_ui" text null, "search_terms" text[] not null, "status" text not null, "approved_by" text null, "approved_at" timestamptz null, "assembled_at" timestamptz null, primary key ("id"));`);
    this.addSql(`alter table "topic" add constraint "topic_slug_unique" unique ("slug");`);
    this.addSql(`alter table "topic" add constraint "topic_type_check" check ("topic_type" in ('procedure', 'condition', 'medication'));`);
    this.addSql(`alter table "topic" add constraint "topic_status_check" check ("status" in ('draft', 'approved'));`);

    this.addSql(`create table "topic_evidence" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "topic_id" uuid not null, "item_key" text not null, "item_kind" text not null, "chunk_id" uuid not null, "document_id" uuid not null, "rank" int not null, "score" double precision not null, primary key ("id"));`);
    this.addSql(`alter table "topic_evidence" add constraint "topic_evidence_topic_id_foreign" foreign key ("topic_id") references "topic" ("id") on update cascade on delete cascade;`);
    this.addSql(`alter table "topic_evidence" add constraint "topic_evidence_chunk_id_foreign" foreign key ("chunk_id") references "document_chunk" ("id") on update cascade on delete cascade;`);
    this.addSql(`alter table "topic_evidence" add constraint "topic_evidence_document_id_foreign" foreign key ("document_id") references "document" ("id") on update cascade on delete cascade;`);
    this.addSql(`create index "topic_evidence_topic_item_index" on "topic_evidence" ("topic_id", "item_key");`);

    this.addSql(`create table "quantitative_fact" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "topic_id" uuid not null, "item_key" text not null, "chunk_id" uuid not null, "document_id" uuid not null, "raw" text not null, "low" double precision not null, "high" double precision not null, "unit" text not null, "statistic" text null, "sentence" text not null, "population" text null, "technique" text null, "review_status" text not null, primary key ("id"));`);
    this.addSql(`alter table "quantitative_fact" add constraint "quantitative_fact_topic_id_foreign" foreign key ("topic_id") references "topic" ("id") on update cascade on delete cascade;`);
    this.addSql(`alter table "quantitative_fact" add constraint "quantitative_fact_chunk_id_foreign" foreign key ("chunk_id") references "document_chunk" ("id") on update cascade on delete cascade;`);
    this.addSql(`alter table "quantitative_fact" add constraint "quantitative_fact_document_id_foreign" foreign key ("document_id") references "document" ("id") on update cascade on delete cascade;`);
    this.addSql(`alter table "quantitative_fact" add constraint "quantitative_fact_review_status_check" check ("review_status" in ('unreviewed', 'verified', 'rejected'));`);
    this.addSql(`create index "quantitative_fact_topic_item_index" on "quantitative_fact" ("topic_id", "item_key");`);

    this.addSql(`create table "case_study" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "topic_id" uuid not null, "document_id" uuid not null, "diagnosis" text not null, "relevance_reason" text not null, "presentation" text null, "diagnosis_text" text null, "management" text null, "outcome" text null, primary key ("id"));`);
    this.addSql(`alter table "case_study" add constraint "case_study_topic_id_foreign" foreign key ("topic_id") references "topic" ("id") on update cascade on delete cascade;`);
    this.addSql(`alter table "case_study" add constraint "case_study_document_id_foreign" foreign key ("document_id") references "document" ("id") on update cascade on delete cascade;`);
    this.addSql(`create unique index "case_study_topic_document_unique" on "case_study" ("topic_id", "document_id");`);

    // Working example topic. Draft: its question framework (procedure-v1) is
    // not yet clinician-approved, so the page is not for clinical use.
    this.addSql(`insert into "topic" ("id", "created_at", "updated_at", "slug", "topic_type", "title", "framework_key", "mesh_descriptor_ui", "search_terms", "status") values (gen_random_uuid(), now(), now(), 'laparoscopic-cholecystectomy', 'procedure', 'Laparoscopic cholecystectomy', 'procedure-v1', 'D017081', array['laparoscopic cholecystectomy', 'cholecystectomy, laparoscopic'], 'draft');`);
  }

  override down(): void | Promise<void> {
    this.addSql(`drop table if exists "case_study" cascade;`);
    this.addSql(`drop table if exists "quantitative_fact" cascade;`);
    this.addSql(`drop table if exists "topic_evidence" cascade;`);
    this.addSql(`drop table if exists "topic" cascade;`);
  }

}

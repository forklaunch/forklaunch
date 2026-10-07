import { Migration } from '@mikro-orm/migrations';

export class Migration00000000000000 extends Migration {
  override async up(): Promise<void> {
    this.addSql(
      `create table "sms_record" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "to" text not null, "body" text not null, "status" text not null, "provider_message_id" text null, "error" text null, "metadata" jsonb null, constraint "sms_record_pkey" primary key ("id"));`
    );
  }

  override async down(): Promise<void> {
    this.addSql(`drop table if exists "sms_record" cascade;`);
  }
}

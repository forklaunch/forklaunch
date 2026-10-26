import { Migration } from '@mikro-orm/migrations';

export class Migration00000000000000 extends Migration {
  override name = 'Migration00000000000000';

  override up(): void | Promise<void> {
    this.addSql(
      `create table "invitation" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "organization_id" text not null, "email" text not null, "role" text null, "status" text not null default 'pending', "inviter_id" text not null, "team_id" text null, "expires_at" timestamptz not null, primary key ("id"));`
    );

    this.addSql(
      `create table "jwks" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "encryption_policy" text null, "public_key" text not null, "private_key" text not null, "expires_at" timestamptz null, "alg" text null, "crv" text null, primary key ("id"));`
    );

    this.addSql(
      `create table "member" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "organization_id" text not null, "user_id" text not null, "role" text not null default 'member', primary key ("id"));`
    );

    this.addSql(
      `create table "organization" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "name" text not null, "slug" text not null, "logo" text null, "metadata" jsonb null, "domain" text null, "subscription" text null, "status" text null, primary key ("id"));`
    );
    this.addSql(
      `alter table "organization" add constraint "organization_slug_unique" unique ("slug");`
    );

    this.addSql(
      `create table "organization_role" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "organization_id" text not null, "role" text not null, "permission" text not null, primary key ("id"));`
    );

    this.addSql(
      `create table "team" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "name" text not null, "member_count" int not null default 0, "organization_id" text not null, primary key ("id"));`
    );

    this.addSql(
      `create table "team_member" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "team_id" text not null, "user_id" text not null, "membership_key" text null, primary key ("id"));`
    );

    this.addSql(
      `create table "user" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "email" text not null, "email_verified" boolean not null, "name" text not null, "first_name" text not null, "last_name" text not null, "image" text null, "phone_number" text null, "subscription" text null, "provider_fields" jsonb null, primary key ("id"));`
    );
    this.addSql(
      `alter table "user" add constraint "user_email_unique" unique ("email");`
    );
    this.addSql(
      `alter table "user" add constraint "user_subscription_unique" unique ("subscription");`
    );

    this.addSql(
      `create table "session" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "user_id" uuid not null, "token" text not null, "expires_at" timestamptz not null, "ip_address" text null, "user_agent" text null, "active_organization_id" text null, "active_team_id" text null, primary key ("id"));`
    );

    this.addSql(
      `create table "account" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "encryption_policy" text null, "user_id" uuid not null, "account_id" text not null, "provider_id" text not null, "access_token" text null, "refresh_token" text null, "access_token_expires_at" timestamptz null, "refresh_token_expires_at" timestamptz null, "scope" text null, "issuer" text null, "id_token" text null, "password" text null, primary key ("id"));`
    );

    this.addSql(
      `create table "verification" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "identifier" text not null, "value" text not null, "expires_at" timestamptz not null, primary key ("id"));`
    );

    this.addSql(
      `alter table "session" add constraint "session_user_id_foreign" foreign key ("user_id") references "user" ("id");`
    );

    this.addSql(
      `alter table "account" add constraint "account_user_id_foreign" foreign key ("user_id") references "user" ("id");`
    );
  }
}

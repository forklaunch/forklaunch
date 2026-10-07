import { Migration } from '@mikro-orm/migrations';

export class Migration20260814142848 extends Migration {

  override name = 'Migration20260814142848';

  override up(): void | Promise<void> {
    this.addSql(`create table "cart" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "customer_id" text null, "status" text not null, "items" jsonb not null, primary key ("id"));`);

    this.addSql(`create table "inventory" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "variant_id" text not null, "stock" int not null, primary key ("id"));`);
    this.addSql(`alter table "inventory" add constraint "inventory_variant_id_unique" unique ("variant_id");`);

    this.addSql(`create table "order" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "customer_id" text null, "cart_id" text null, "status" text not null, "items" jsonb not null, "shipping_address" text not null, "subtotal_cents" int not null, "discount_cents" int not null, "tax_cents" int not null, "tax_breakdown" jsonb not null, "shipping_cents" int not null, "gift_card_cents" int not null, "total_cents" int not null, primary key ("id"));`);
    this.addSql(`create index "order_cart_id_index" on "order" ("cart_id");`);
    this.addSql(`create unique index "order_cart_id_pending_unique" on "public"."order" ("cart_id") where "status" = 'pending';`);
    this.addSql(`alter table "order" add constraint "order_status_check" check ("status" in ('pending', 'paid', 'fulfilled', 'shipped', 'delivered', 'cancelled'));`);

    this.addSql(`create table "order_event_record" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "order_id" text not null, "from_status" text not null, "to_status" text not null, "items" jsonb not null, "processed" boolean not null, "retry_count" int not null, primary key ("id"));`);

    this.addSql(`create table "payment" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "order_id" text not null, "amount_cents" int not null, "currency" text not null, "status" text not null, "provider_ref" text null, primary key ("id"));`);
    this.addSql(`alter table "payment" add constraint "payment_provider_ref_unique" unique ("provider_ref");`);
    this.addSql(`alter table "payment" add constraint "payment_status_check" check ("status" in ('pending', 'succeeded', 'failed'));`);

    this.addSql(`create table "product" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "external_id" text not null, "handle" text not null, "source_url" text null, "title" text not null, "description_html" text null, "vendor" text null, "product_type" text null, "tags" varchar(255)[] null, "options" jsonb null, "images" jsonb null, primary key ("id"));`);
    this.addSql(`alter table "product" add constraint "product_external_id_unique" unique ("external_id");`);
    this.addSql(`alter table "product" add constraint "product_handle_unique" unique ("handle");`);

    this.addSql(`create table "variant" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "product_id" text not null, "external_id" text not null, "sku" text null, "title" text not null, "option_values" jsonb null, "price_cents" int not null, "compare_at_price_cents" int null, "requires_shipping" boolean not null, primary key ("id"));`);
    this.addSql(`alter table "variant" add constraint "variant_external_id_unique" unique ("external_id");`);

    this.addSql(`create table "webhook_event" ("id" uuid not null, "created_at" timestamptz not null, "updated_at" timestamptz not null, "retention_anonymized_at" timestamptz null, "provider" text not null, "provider_event_id" text not null, "event_type" text not null, "processed" boolean not null, primary key ("id"));`);
    this.addSql(`alter table "webhook_event" add constraint "webhook_event_provider_provider_event_id_unique" unique ("provider", "provider_event_id");`);
  }

}

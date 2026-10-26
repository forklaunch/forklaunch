CREATE TABLE fl_ecommerce.records (
    tenant_id text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('product','variant','inventory','cart','order','payment')),
    id text NOT NULL,
    data jsonb NOT NULL CHECK (jsonb_typeof(data) = 'object' AND data->>'id' = id),
    PRIMARY KEY (tenant_id,kind,id)
);
CREATE UNIQUE INDEX product_external_id ON fl_ecommerce.records (tenant_id,(data->>'externalId')) WHERE kind='product';
CREATE UNIQUE INDEX product_handle ON fl_ecommerce.records (tenant_id,(data->>'handle')) WHERE kind='product';
CREATE UNIQUE INDEX variant_external_id ON fl_ecommerce.records (tenant_id,(data->>'externalId')) WHERE kind='variant';
CREATE UNIQUE INDEX inventory_variant_id ON fl_ecommerce.records (tenant_id,(data->>'variantId')) WHERE kind='inventory';
CREATE UNIQUE INDEX payment_provider_ref ON fl_ecommerce.records (tenant_id,(data->>'providerRef')) WHERE kind='payment';
CREATE UNIQUE INDEX pending_order_cart_id ON fl_ecommerce.records (tenant_id,(data->>'cartId')) WHERE kind='order' AND data->>'status'='pending';
CREATE INDEX variants_by_product ON fl_ecommerce.records (tenant_id,(data->>'productId')) WHERE kind='variant';
CREATE INDEX orders_by_customer ON fl_ecommerce.records (tenant_id,(data->>'customerId')) WHERE kind='order';
CREATE TABLE fl_ecommerce.pii_read_audit (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    record_id text NOT NULL,
    actor_id text,
    correlation_id text NOT NULL,
    operation text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_ecommerce.records ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_ecommerce.records FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_records ON fl_ecommerce.records
    USING (tenant_id = current_setting('forklaunch.tenant_id',true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id',true));
ALTER TABLE fl_ecommerce.pii_read_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_ecommerce.pii_read_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_audit ON fl_ecommerce.pii_read_audit
    USING (tenant_id = current_setting('forklaunch.tenant_id',true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id',true));

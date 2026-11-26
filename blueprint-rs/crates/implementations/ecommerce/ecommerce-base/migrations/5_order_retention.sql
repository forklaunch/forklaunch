-- Separate markers survive ordinary DTO-based order updates. The FK removes a
-- marker whenever its order is deleted, including compliance erasure.
CREATE TABLE fl_ecommerce.order_retention (
    tenant_id text NOT NULL,
    kind text NOT NULL DEFAULT 'order' CHECK (kind = 'order'),
    order_id text NOT NULL,
    anonymized_at timestamptz NOT NULL,
    PRIMARY KEY (tenant_id,order_id),
    FOREIGN KEY (tenant_id,kind,order_id) REFERENCES fl_ecommerce.records(tenant_id,kind,id) ON DELETE CASCADE
);
ALTER TABLE fl_ecommerce.order_retention ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_ecommerce.order_retention FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_retention ON fl_ecommerce.order_retention
    USING (tenant_id = current_setting('forklaunch.tenant_id',true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id',true));

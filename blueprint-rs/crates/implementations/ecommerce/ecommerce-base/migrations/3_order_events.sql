-- Inventory receipts commit in the same transaction as all stock deltas.
CREATE TABLE fl_ecommerce.inventory_event_receipts (
    tenant_id text NOT NULL,
    event_id text NOT NULL,
    fingerprint text NOT NULL,
    applied_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id,event_id)
);
ALTER TABLE fl_ecommerce.inventory_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_ecommerce.inventory_event_receipts FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_inventory_receipts ON fl_ecommerce.inventory_event_receipts
    USING (tenant_id = current_setting('forklaunch.tenant_id',true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id',true));

-- Metadata-only receipts: raw event payloads and customer data are not retained.
CREATE TABLE fl_billing.webhook_receipts (
    tenant_id text NOT NULL,
    provider text NOT NULL,
    event_id text NOT NULL,
    event_type text NOT NULL,
    processed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(tenant_id,provider,event_id)
);
ALTER TABLE fl_billing.webhook_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.webhook_receipts FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_webhook_tenant ON fl_billing.webhook_receipts
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE TABLE fl_billing.platform_event_nonces (
    tenant_id text NOT NULL,
    nonce text NOT NULL,
    expires_at timestamptz NOT NULL,
    PRIMARY KEY(tenant_id,nonce)
);
CREATE INDEX billing_platform_nonce_expiry ON fl_billing.platform_event_nonces(tenant_id,expires_at);
ALTER TABLE fl_billing.platform_event_nonces ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.platform_event_nonces FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_platform_nonce_tenant ON fl_billing.platform_event_nonces
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

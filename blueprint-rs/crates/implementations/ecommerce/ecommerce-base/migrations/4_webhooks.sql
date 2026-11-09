CREATE TABLE fl_ecommerce.webhook_events (
    tenant_id text NOT NULL,
    provider text NOT NULL,
    provider_event_id text NOT NULL,
    event_type text NOT NULL,
    processed boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(tenant_id,provider,provider_event_id)
);
ALTER TABLE fl_ecommerce.webhook_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_ecommerce.webhook_events FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_webhooks ON fl_ecommerce.webhook_events
    USING (tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK (tenant_id=current_setting('forklaunch.tenant_id',true));

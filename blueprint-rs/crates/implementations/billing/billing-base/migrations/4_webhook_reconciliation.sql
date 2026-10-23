-- Commit intent before effects; retain ambiguous attempts until audited resolution.
CREATE TABLE fl_billing.webhook_attempts (
    tenant_id text NOT NULL,
    provider text NOT NULL,
    event_id text NOT NULL,
    event_type text NOT NULL,
    attempt_id uuid NOT NULL,
    started_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(tenant_id,provider,event_id)
);
CREATE INDEX billing_webhook_pending ON fl_billing.webhook_attempts(tenant_id,started_at,provider,event_id);
ALTER TABLE fl_billing.webhook_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.webhook_attempts FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_webhook_attempt_tenant ON fl_billing.webhook_attempts
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE TABLE fl_billing.webhook_reconciliation_audit (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    provider text NOT NULL,
    event_id text NOT NULL,
    event_type text NOT NULL,
    attempt_id uuid NOT NULL,
    resolution text NOT NULL CHECK(resolution IN ('applied','retry_allowed')),
    actor_id text NOT NULL,
    correlation_id text NOT NULL,
    resolved_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_billing.webhook_reconciliation_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.webhook_reconciliation_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_webhook_resolution_tenant ON fl_billing.webhook_reconciliation_audit
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

CREATE TABLE fl_billing.records (
    tenant_id text NOT NULL,
    scope text NOT NULL CHECK(scope IN ('durable','cache')),
    kind text NOT NULL CHECK(kind IN ('plan','subscription','checkout_session','payment_link','billing_portal')),
    id text NOT NULL,
    data jsonb NOT NULL,
    expires_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(tenant_id,scope,kind,id),
    CHECK((scope='durable' AND expires_at IS NULL) OR (scope='cache' AND expires_at IS NOT NULL))
);
CREATE UNIQUE INDEX billing_external_id ON fl_billing.records(tenant_id,kind,(data->>'externalId'))
    WHERE scope='durable' AND kind IN ('plan','subscription');
CREATE INDEX billing_party ON fl_billing.records(tenant_id,kind,(data->>'partyId'),(data->>'partyType')) WHERE scope='durable' AND kind='subscription';
CREATE INDEX billing_cache_expiry ON fl_billing.records(tenant_id,expires_at) WHERE scope='cache';
ALTER TABLE fl_billing.records ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.records FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_records_tenant ON fl_billing.records
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE TABLE fl_billing.sensitive_read_audit (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    scope text NOT NULL,
    kind text NOT NULL,
    record_id text NOT NULL,
    actor_id text,
    correlation_id text NOT NULL,
    operation text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_billing.sensitive_read_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.sensitive_read_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_audit_tenant ON fl_billing.sensitive_read_audit
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

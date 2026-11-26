-- Explicit administrative aliases only. Existing record keys/ciphertext stay intact.
CREATE TABLE fl_billing.provider_bindings (
    tenant_id text NOT NULL,
    kind text NOT NULL CHECK(kind IN ('checkout_session','payment_link','billing_portal')),
    scope text NOT NULL DEFAULT 'durable' CHECK(scope='durable'),
    public_id uuid NOT NULL,
    binding_id uuid NOT NULL,
    storage_id text NOT NULL,
    deployment_label text NOT NULL,
    account_id text NOT NULL,
    livemode boolean NOT NULL,
    provider_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(tenant_id,kind,public_id),
    UNIQUE(tenant_id,kind,storage_id),
    UNIQUE(tenant_id,deployment_label,account_id,livemode,kind,provider_id),
    FOREIGN KEY(tenant_id,scope,kind,storage_id)
        REFERENCES fl_billing.records(tenant_id,scope,kind,id) ON DELETE RESTRICT
);
ALTER TABLE fl_billing.provider_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.provider_bindings FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_bindings_tenant ON fl_billing.provider_bindings
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

CREATE TABLE fl_billing.provider_binding_audit (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    binding_id uuid NOT NULL,
    public_id uuid NOT NULL,
    kind text NOT NULL,
    storage_id text NOT NULL,
    deployment_label text NOT NULL,
    account_id text NOT NULL,
    livemode boolean NOT NULL,
    provider_id text NOT NULL,
    operation text NOT NULL CHECK(operation IN ('provision','revoke')),
    actor_id text NOT NULL,
    correlation_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_billing.provider_binding_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.provider_binding_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_binding_audit_read ON fl_billing.provider_binding_audit
    FOR SELECT USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY billing_binding_audit_append ON fl_billing.provider_binding_audit
    FOR INSERT
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

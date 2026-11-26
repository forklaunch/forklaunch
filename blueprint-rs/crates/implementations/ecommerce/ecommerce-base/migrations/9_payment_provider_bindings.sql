-- Explicit administrative assertions only: no provider verification or backfill.
CREATE TABLE fl_ecommerce.payment_provider_bindings (
    tenant_id text NOT NULL,
    payment_id text NOT NULL,
    payment_kind text NOT NULL DEFAULT 'payment' CHECK(payment_kind='payment'),
    provider text NOT NULL CHECK(provider IN ('stripe','paypal')),
    deployment_label text NOT NULL CHECK(length(deployment_label) BETWEEN 1 AND 128),
    merchant_id text NOT NULL CHECK(length(merchant_id) BETWEEN 1 AND 128),
    livemode boolean NOT NULL,
    generation uuid NOT NULL,
    provider_ref text NOT NULL CHECK(length(provider_ref) BETWEEN 1 AND 128),
    order_id text NOT NULL,
    order_kind text NOT NULL DEFAULT 'order' CHECK(order_kind='order'),
    amount_cents bigint NOT NULL CHECK(amount_cents>0),
    currency text NOT NULL CHECK(currency ~ '^[A-Z]{3}$'),
    PRIMARY KEY(tenant_id,payment_id),
    FOREIGN KEY(tenant_id,payment_kind,payment_id) REFERENCES fl_ecommerce.records(tenant_id,kind,id) ON DELETE CASCADE,
    FOREIGN KEY(tenant_id,order_kind,order_id) REFERENCES fl_ecommerce.records(tenant_id,kind,id) ON DELETE CASCADE,
    UNIQUE(tenant_id,provider,deployment_label,merchant_id,livemode,provider_ref)
);
CREATE TABLE fl_ecommerce.payment_provider_binding_audit (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    payment_id text NOT NULL,
    generation uuid NOT NULL,
    operation text NOT NULL CHECK(operation IN ('provision','revoke')),
    provider text NOT NULL,
    deployment_label text NOT NULL,
    merchant_id text NOT NULL,
    livemode boolean NOT NULL,
    provider_ref text NOT NULL,
    order_id text NOT NULL,
    amount_cents bigint NOT NULL,
    currency text NOT NULL,
    actor_id text NOT NULL CHECK(length(trim(actor_id))>0),
    correlation_id text NOT NULL CHECK(length(trim(correlation_id))>0),
    recorded_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_ecommerce.payment_provider_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_ecommerce.payment_provider_bindings FORCE ROW LEVEL SECURITY;
CREATE POLICY payment_provider_binding_tenant ON fl_ecommerce.payment_provider_bindings
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));
ALTER TABLE fl_ecommerce.payment_provider_binding_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_ecommerce.payment_provider_binding_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY payment_provider_binding_audit_read ON fl_ecommerce.payment_provider_binding_audit FOR SELECT
    USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY payment_provider_binding_audit_insert ON fl_ecommerce.payment_provider_binding_audit FOR INSERT
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

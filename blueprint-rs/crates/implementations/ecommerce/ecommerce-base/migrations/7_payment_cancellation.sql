CREATE TABLE fl_ecommerce.payment_cancellation_audit (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    payment_id text NOT NULL,
    previous_status text NOT NULL,
    provider text NOT NULL,
    provider_ref text NOT NULL,
    canceled_at bigint NOT NULL CHECK(canceled_at>0),
    amount_cents bigint NOT NULL CHECK(amount_cents>0),
    currency text NOT NULL,
    actor_id text NOT NULL,
    correlation_id text NOT NULL,
    reconciled_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX ecommerce_payment_cancellation_lookup ON fl_ecommerce.payment_cancellation_audit(tenant_id,payment_id,provider_ref);
ALTER TABLE fl_ecommerce.payment_cancellation_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_ecommerce.payment_cancellation_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY ecommerce_payment_cancellation_tenant ON fl_ecommerce.payment_cancellation_audit
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

-- Additive operator receipts; no inference/backfill from historic repairs.
CREATE TABLE fl_ecommerce.scoped_paypal_cancellation_receipts (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    cancellation_audit_id uuid NOT NULL UNIQUE REFERENCES fl_ecommerce.payment_cancellation_audit(id),
    payment_id text NOT NULL,
    order_id text NOT NULL,
    order_version text NOT NULL,
    binding_generation uuid NOT NULL,
    provider text NOT NULL CHECK(provider='paypal'),
    deployment_label text NOT NULL,
    merchant_id text NOT NULL,
    livemode boolean NOT NULL,
    provider_ref text NOT NULL,
    amount_cents bigint NOT NULL CHECK(amount_cents>0),
    currency text NOT NULL CHECK(currency ~ '^[A-Z]{3}$'),
    actor_id text NOT NULL CHECK(length(trim(actor_id))>0),
    correlation_id text NOT NULL CHECK(length(trim(correlation_id))>0),
    recorded_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(tenant_id,provider,deployment_label,merchant_id,livemode,provider_ref)
);
ALTER TABLE fl_ecommerce.scoped_paypal_cancellation_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_ecommerce.scoped_paypal_cancellation_receipts FORCE ROW LEVEL SECURITY;
CREATE POLICY scoped_paypal_cancellation_receipt_read ON fl_ecommerce.scoped_paypal_cancellation_receipts FOR SELECT
    USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY scoped_paypal_cancellation_receipt_insert ON fl_ecommerce.scoped_paypal_cancellation_receipts FOR INSERT
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

-- Additive operator receipts; no inference/backfill from historic repairs.
CREATE TABLE fl_ecommerce.scoped_payment_capture_receipts (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    reconciliation_audit_id uuid NOT NULL UNIQUE REFERENCES fl_ecommerce.payment_reconciliation_audit(id),
    payment_id text NOT NULL,
    order_id text NOT NULL,
    order_version text NOT NULL,
    binding_generation uuid NOT NULL,
    provider text NOT NULL CHECK(provider='stripe'),
    deployment_label text NOT NULL,
    merchant_id text NOT NULL,
    livemode boolean NOT NULL,
    provider_ref text NOT NULL,
    amount_cents bigint NOT NULL CHECK(amount_cents>0),
    currency text NOT NULL CHECK(currency ~ '^[A-Z]{3}$'),
    capture_id text NOT NULL CHECK(length(trim(capture_id))>0),
    balance_transaction_id text NOT NULL CHECK(length(trim(balance_transaction_id))>0),
    actor_id text NOT NULL CHECK(length(trim(actor_id))>0),
    correlation_id text NOT NULL CHECK(length(trim(correlation_id))>0),
    recorded_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(tenant_id,payment_id,binding_generation,provider_ref,capture_id)
);
ALTER TABLE fl_ecommerce.scoped_payment_capture_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_ecommerce.scoped_payment_capture_receipts FORCE ROW LEVEL SECURITY;
CREATE POLICY scoped_payment_capture_receipt_read ON fl_ecommerce.scoped_payment_capture_receipts FOR SELECT
    USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY scoped_payment_capture_receipt_insert ON fl_ecommerce.scoped_payment_capture_receipts FOR INSERT
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

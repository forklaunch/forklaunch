-- Some providers attest terminal void without a cancellation timestamp.
-- reconciled_at continues to record the local observation; existing values and
-- the positive-timestamp CHECK constraint remain unchanged.
ALTER TABLE fl_ecommerce.payment_cancellation_audit ALTER COLUMN canceled_at DROP NOT NULL;

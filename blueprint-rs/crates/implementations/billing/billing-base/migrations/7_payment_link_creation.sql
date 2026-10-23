-- Opt-in direct creation journal. No legacy row/key/ciphertext migration.
CREATE TABLE fl_billing.payment_link_creation_intents (
    tenant_id text NOT NULL,
    operation_id uuid NOT NULL,
    public_id uuid NOT NULL,
    reservation_id uuid NOT NULL,
    actor_id text NOT NULL,
    deployment_label text NOT NULL,
    account_id text NOT NULL,
    livemode boolean NOT NULL,
    api_version text NOT NULL,
    payload_hash text NOT NULL,
    idempotency_key text NOT NULL,
    intent_ciphertext text NOT NULL,
    state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','completed')),
    receipt_ciphertext text,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY(tenant_id,operation_id),
    UNIQUE(tenant_id,public_id),
    UNIQUE(idempotency_key),
    CHECK ((state='pending' AND receipt_ciphertext IS NULL) OR
           (state='completed' AND receipt_ciphertext IS NOT NULL))
);
ALTER TABLE fl_billing.payment_link_creation_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.payment_link_creation_intents FORCE ROW LEVEL SECURITY;
CREATE POLICY payment_link_creation_read ON fl_billing.payment_link_creation_intents FOR SELECT
    USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY payment_link_creation_insert ON fl_billing.payment_link_creation_intents FOR INSERT
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true) AND state='pending' AND receipt_ciphertext IS NULL);
CREATE POLICY payment_link_creation_complete ON fl_billing.payment_link_creation_intents FOR UPDATE
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE FUNCTION fl_billing.guard_payment_link_creation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF (to_jsonb(NEW)-'state'-'receipt_ciphertext') IS DISTINCT FROM
       (to_jsonb(OLD)-'state'-'receipt_ciphertext') OR OLD.state <> 'pending' OR
       NEW.state <> 'completed' OR NEW.receipt_ciphertext IS NULL THEN
        RAISE EXCEPTION 'Creation intent is immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER payment_link_creation_immutable BEFORE UPDATE ON fl_billing.payment_link_creation_intents
    FOR EACH ROW EXECUTE FUNCTION fl_billing.guard_payment_link_creation();
CREATE TABLE fl_billing.payment_link_creation_audit (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    operation_id uuid NOT NULL,
    actor_id text NOT NULL,
    correlation_id text NOT NULL,
    operation text NOT NULL CHECK(operation IN ('reserve','inspect','complete')),
    created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_billing.payment_link_creation_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.payment_link_creation_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY payment_link_creation_audit_read ON fl_billing.payment_link_creation_audit FOR SELECT
    USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY payment_link_creation_audit_append ON fl_billing.payment_link_creation_audit FOR INSERT
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

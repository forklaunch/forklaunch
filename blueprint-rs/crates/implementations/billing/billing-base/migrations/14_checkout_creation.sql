-- Stage A only: immutable reservations and durable dependency barriers.
-- No dispatch, completion, release, TTL expiry or financial operation is exposed.
CREATE TABLE fl_billing.checkout_creation_intents (
 tenant_id text NOT NULL, operation_id uuid NOT NULL, public_id uuid NOT NULL,
 reservation_id uuid NOT NULL, actor_id text NOT NULL,
 deployment_label text NOT NULL, account_id text NOT NULL, livemode boolean NOT NULL,
 api_version text NOT NULL, payload_hash text NOT NULL, idempotency_key text NOT NULL,
 intent_ciphertext text NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(tenant_id,operation_id), UNIQUE(tenant_id,public_id), UNIQUE(idempotency_key)
);
ALTER TABLE fl_billing.checkout_creation_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.checkout_creation_intents FORCE ROW LEVEL SECURITY;
CREATE POLICY checkout_creation_read ON fl_billing.checkout_creation_intents FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY checkout_creation_insert ON fl_billing.checkout_creation_intents FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

CREATE TABLE fl_billing.checkout_dependency_claims (
 tenant_id text NOT NULL, operation_id uuid NOT NULL,
 dependency_kind text NOT NULL CHECK(dependency_kind IN
 ('customer_record','plan_record','customer_binding','plan_binding')),
 dependency_id text NOT NULL,
 PRIMARY KEY(tenant_id,operation_id,dependency_kind),
 FOREIGN KEY(tenant_id,operation_id)
 REFERENCES fl_billing.checkout_creation_intents(tenant_id,operation_id) ON DELETE RESTRICT
);
ALTER TABLE fl_billing.checkout_dependency_claims ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.checkout_dependency_claims FORCE ROW LEVEL SECURITY;
CREATE POLICY checkout_dependency_read ON fl_billing.checkout_dependency_claims FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY checkout_dependency_insert ON fl_billing.checkout_dependency_claims FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

-- Claim trigger takes the actual dependency row lock and increments the private
-- shared counter. Its PUBLIC EXECUTE is revoked; no runtime counter authority.
CREATE TRIGGER checkout_dependency_claim AFTER INSERT ON fl_billing.checkout_dependency_claims
 FOR EACH ROW EXECUTE FUNCTION fl_billing.claim_subscription_dependency();
CREATE FUNCTION fl_billing.deny_checkout_journal_mutation() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $function$
BEGIN RAISE EXCEPTION 'Checkout reservation is immutable'; END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.deny_checkout_journal_mutation() FROM PUBLIC;
CREATE TRIGGER checkout_intent_immutable BEFORE UPDATE OR DELETE OR TRUNCATE
 ON fl_billing.checkout_creation_intents FOR EACH STATEMENT
 EXECUTE FUNCTION fl_billing.deny_checkout_journal_mutation();
CREATE TRIGGER checkout_claim_immutable BEFORE UPDATE OR DELETE OR TRUNCATE
 ON fl_billing.checkout_dependency_claims FOR EACH STATEMENT
 EXECUTE FUNCTION fl_billing.deny_checkout_journal_mutation();
CREATE TABLE fl_billing.checkout_creation_audit (
 id uuid PRIMARY KEY, tenant_id text NOT NULL, operation_id uuid NOT NULL,
 actor_id text NOT NULL, correlation_id text NOT NULL,
 operation text NOT NULL CHECK(operation IN ('reserve','inspect')),
 created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_billing.checkout_creation_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.checkout_creation_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY checkout_audit_read ON fl_billing.checkout_creation_audit FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY checkout_audit_insert ON fl_billing.checkout_creation_audit FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

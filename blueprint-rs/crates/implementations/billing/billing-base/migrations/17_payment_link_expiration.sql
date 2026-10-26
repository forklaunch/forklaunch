-- Explicit metadata-only PaymentLink expiration. Dedicated claims namespace.
-- Runtime can reserve, never complete/release; no TTL release or automatic retry.
CREATE TABLE fl_billing.payment_link_expiration_intents (
 tenant_id text NOT NULL, operation_id uuid NOT NULL, public_id uuid NOT NULL,
 reservation_id uuid NOT NULL, actor_id text NOT NULL,
 deployment_label text NOT NULL, account_id text NOT NULL, livemode boolean NOT NULL,
 api_version text NOT NULL, payload_hash text NOT NULL, idempotency_key text NOT NULL,
 intent_ciphertext text NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(tenant_id,operation_id), UNIQUE(tenant_id,public_id), UNIQUE(idempotency_key)
);
ALTER TABLE fl_billing.payment_link_expiration_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.payment_link_expiration_intents FORCE ROW LEVEL SECURITY;
CREATE POLICY payment_link_expiration_read ON fl_billing.payment_link_expiration_intents FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY payment_link_expiration_insert ON fl_billing.payment_link_expiration_intents FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

CREATE TABLE fl_billing.payment_link_expiration_dependency_claims (
 tenant_id text NOT NULL, operation_id uuid NOT NULL,
 dependency_kind text NOT NULL CHECK(dependency_kind IN
 ('payment_link_record','payment_link_binding')),
 dependency_id text NOT NULL,
 PRIMARY KEY(tenant_id,operation_id,dependency_kind),
 FOREIGN KEY(tenant_id,operation_id)
 REFERENCES fl_billing.payment_link_expiration_intents(tenant_id,operation_id) ON DELETE RESTRICT
);
ALTER TABLE fl_billing.payment_link_expiration_dependency_claims ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.payment_link_expiration_dependency_claims FORCE ROW LEVEL SECURITY;
CREATE POLICY payment_link_expiration_dependency_read ON fl_billing.payment_link_expiration_dependency_claims FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY payment_link_expiration_dependency_insert ON fl_billing.payment_link_expiration_dependency_claims FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

-- Private counters prevent a stale SQL statement snapshot from overlooking a
-- reservation that committed while its mutation waited for the authoritative row.
-- UPDATE RETURNING observes the locked current counter, unlike a plain SELECT.
CREATE TABLE fl_billing.payment_link_expiration_dependency_guards (
 tenant_id text NOT NULL, dependency_kind text NOT NULL, dependency_id text NOT NULL,
 claims bigint NOT NULL CHECK(claims>=0),
 PRIMARY KEY(tenant_id,dependency_kind,dependency_id)
);
ALTER TABLE fl_billing.payment_link_expiration_dependency_guards ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.payment_link_expiration_dependency_guards FORCE ROW LEVEL SECURITY;
DO $migration$
BEGIN
 EXECUTE format('CREATE POLICY payment_link_expiration_guard_owner ON fl_billing.payment_link_expiration_dependency_guards TO %I USING(true) WITH CHECK(true)',current_user);
END;
$migration$;
REVOKE ALL ON fl_billing.payment_link_expiration_dependency_guards FROM PUBLIC;
-- Runtime MUST be a distinct non-inheriting role, not a member of migration owner.
-- Even broad runtime table grants do not receive the private owner RLS policy.

-- Guard rows exist before any claim, including for unclaimed dependencies.
-- Thus a waiting UPDATE sees a changed counter through PostgreSQL row locking
-- (or a serialization failure at stronger isolation), never an invisible insert.
DO $migration$
DECLARE target text;
BEGIN
 FOREACH target IN ARRAY ARRAY['records','provider_bindings'] LOOP
  EXECUTE format('CREATE POLICY payment_link_expiration_migration_read ON fl_billing.%I FOR SELECT TO %I USING(true)',target,current_user);
 END LOOP;
END;
$migration$;
INSERT INTO fl_billing.payment_link_expiration_dependency_guards
 SELECT tenant_id,kind||'_record',id,0 FROM fl_billing.records
 WHERE scope='durable' AND kind='payment_link';
INSERT INTO fl_billing.payment_link_expiration_dependency_guards
 SELECT tenant_id,'payment_link_binding',binding_id::text,0 FROM fl_billing.provider_bindings WHERE kind='payment_link';
DROP POLICY payment_link_expiration_migration_read ON fl_billing.records;
DROP POLICY payment_link_expiration_migration_read ON fl_billing.provider_bindings;
CREATE FUNCTION fl_billing.initialize_payment_link_expiration_guard() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $function$
DECLARE target_kind text; target_id text;
BEGIN
 IF TG_TABLE_NAME='records' THEN
  IF NEW.scope<>'durable' OR NEW.kind<>'payment_link' THEN RETURN NEW; END IF;
  target_kind:=NEW.kind||'_record'; target_id:=NEW.id;
 ELSE
  IF NEW.kind<>'payment_link' THEN RETURN NEW; END IF;
  target_kind:='payment_link_binding'; target_id:=NEW.binding_id::text;
 END IF;
 INSERT INTO fl_billing.payment_link_expiration_dependency_guards(tenant_id,dependency_kind,dependency_id,claims)
 VALUES(NEW.tenant_id,target_kind,target_id,0) ON CONFLICT DO NOTHING;
 RETURN NEW;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.initialize_payment_link_expiration_guard() FROM PUBLIC;
CREATE TRIGGER payment_link_expiration_record_initialize AFTER INSERT OR UPDATE ON fl_billing.records
 FOR EACH ROW EXECUTE FUNCTION fl_billing.initialize_payment_link_expiration_guard();
CREATE TRIGGER payment_link_expiration_binding_initialize AFTER INSERT OR UPDATE ON fl_billing.provider_bindings
 FOR EACH ROW EXECUTE FUNCTION fl_billing.initialize_payment_link_expiration_guard();

CREATE FUNCTION fl_billing.claim_payment_link_expiration_dependency() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $function$
DECLARE found_id text;
BEGIN
 IF NEW.tenant_id IS DISTINCT FROM current_setting('forklaunch.tenant_id',true) THEN
  RAISE EXCEPTION 'Invalid payment-link expiration dependency tenant';
 END IF;
 IF NEW.dependency_kind='payment_link_record' THEN
  SELECT id INTO found_id FROM fl_billing.records
   WHERE tenant_id=NEW.tenant_id AND scope='durable'
   AND kind=replace(NEW.dependency_kind,'_record','') AND id=NEW.dependency_id FOR UPDATE;
 ELSIF NEW.dependency_kind='payment_link_binding' THEN
  SELECT binding_id::text INTO found_id FROM fl_billing.provider_bindings
   WHERE tenant_id=NEW.tenant_id AND kind='payment_link' AND binding_id::text=NEW.dependency_id FOR UPDATE;
 END IF;
 IF found_id IS NULL THEN RAISE EXCEPTION 'Missing payment-link expiration dependency'; END IF;
 PERFORM 1 FROM fl_billing.payment_link_expiration_intents WHERE tenant_id=NEW.tenant_id AND operation_id=NEW.operation_id AND state='pending' FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Payment-link expiration is not pending'; END IF;
 INSERT INTO fl_billing.payment_link_expiration_dependency_guards(tenant_id,dependency_kind,dependency_id,claims)
 VALUES(NEW.tenant_id,NEW.dependency_kind,NEW.dependency_id,1)
 ON CONFLICT(tenant_id,dependency_kind,dependency_id)
 DO UPDATE SET claims=fl_billing.payment_link_expiration_dependency_guards.claims+1;
 RETURN NEW;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.claim_payment_link_expiration_dependency() FROM PUBLIC;
CREATE TRIGGER payment_link_expiration_dependency_claim AFTER INSERT ON fl_billing.payment_link_expiration_dependency_claims
 FOR EACH ROW EXECUTE FUNCTION fl_billing.claim_payment_link_expiration_dependency();

CREATE FUNCTION fl_billing.guard_payment_link_expiration_dependency() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $function$
DECLARE target_kind text; target_id text; held bigint;
BEGIN
 IF TG_TABLE_NAME='records' THEN
  IF OLD.scope<>'durable' OR OLD.kind<>'payment_link' THEN
   IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
  END IF;
  target_kind:=OLD.kind||'_record'; target_id:=OLD.id;
 ELSE
  IF OLD.kind<>'payment_link' THEN
   IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
  END IF;
  target_kind:='payment_link_binding'; target_id:=OLD.binding_id::text;
 END IF;
 UPDATE fl_billing.payment_link_expiration_dependency_guards g SET claims=g.claims
  WHERE g.tenant_id=OLD.tenant_id AND g.dependency_kind=target_kind
   AND g.dependency_id=target_id RETURNING g.claims INTO held;
 IF held IS NULL OR held>0 THEN RAISE EXCEPTION 'Payment-link expiration dependency is claimed'; END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.guard_payment_link_expiration_dependency() FROM PUBLIC;
CREATE TRIGGER payment_link_expiration_record_barrier BEFORE UPDATE OR DELETE ON fl_billing.records
 FOR EACH ROW EXECUTE FUNCTION fl_billing.guard_payment_link_expiration_dependency();
CREATE TRIGGER payment_link_expiration_binding_barrier BEFORE UPDATE OR DELETE ON fl_billing.provider_bindings
 FOR EACH ROW EXECUTE FUNCTION fl_billing.guard_payment_link_expiration_dependency();

CREATE FUNCTION fl_billing.deny_payment_link_expiration_journal_mutation() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $function$
BEGIN RAISE EXCEPTION 'Payment-link expiration reservation is immutable; no release is implemented'; END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.deny_payment_link_expiration_journal_mutation() FROM PUBLIC;
CREATE TRIGGER payment_link_expiration_intent_immutable BEFORE UPDATE OR DELETE OR TRUNCATE
 ON fl_billing.payment_link_expiration_intents FOR EACH STATEMENT
 EXECUTE FUNCTION fl_billing.deny_payment_link_expiration_journal_mutation();
CREATE TRIGGER payment_link_expiration_guard_no_truncate BEFORE TRUNCATE
 ON fl_billing.payment_link_expiration_dependency_guards FOR EACH STATEMENT
 EXECUTE FUNCTION fl_billing.deny_payment_link_expiration_journal_mutation();
CREATE TRIGGER payment_link_expiration_claim_immutable BEFORE UPDATE OR DELETE OR TRUNCATE
 ON fl_billing.payment_link_expiration_dependency_claims FOR EACH STATEMENT
 EXECUTE FUNCTION fl_billing.deny_payment_link_expiration_journal_mutation();
CREATE FUNCTION fl_billing.guard_payment_link_expiration_truncate() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $function$
BEGIN
 IF EXISTS(SELECT 1 FROM fl_billing.payment_link_expiration_dependency_guards WHERE claims>0) THEN
  RAISE EXCEPTION 'Payment-link expiration dependencies are claimed';
 END IF;
 RETURN NULL;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.guard_payment_link_expiration_truncate() FROM PUBLIC;
CREATE TRIGGER payment_link_expiration_record_truncate BEFORE TRUNCATE ON fl_billing.records
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.guard_payment_link_expiration_truncate();
CREATE TRIGGER payment_link_expiration_binding_truncate BEFORE TRUNCATE ON fl_billing.provider_bindings
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.guard_payment_link_expiration_truncate();

CREATE TABLE fl_billing.payment_link_expiration_audit (
 id uuid PRIMARY KEY, tenant_id text NOT NULL, operation_id uuid NOT NULL,
 actor_id text NOT NULL, correlation_id text NOT NULL,
 operation text NOT NULL CHECK(operation IN ('reserve','inspect','complete')),
 created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_billing.payment_link_expiration_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.payment_link_expiration_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY payment_link_expiration_audit_read ON fl_billing.payment_link_expiration_audit FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY payment_link_expiration_audit_insert ON fl_billing.payment_link_expiration_audit FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

-- Explicit separate trusted Payment-link expiration publisher; ordinary runtime cannot complete/release.
ALTER TABLE fl_billing.payment_link_expiration_intents
 ADD COLUMN state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','completed')),
 ADD COLUMN receipt_ciphertext text,
 ADD CONSTRAINT payment_link_expiration_receipt_state CHECK
 ((state='pending' AND receipt_ciphertext IS NULL) OR
  (state='completed' AND receipt_ciphertext IS NOT NULL));
DROP POLICY payment_link_expiration_insert ON fl_billing.payment_link_expiration_intents;
CREATE POLICY payment_link_expiration_insert ON fl_billing.payment_link_expiration_intents FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true) AND state='pending' AND receipt_ciphertext IS NULL);
ALTER TABLE fl_billing.payment_link_expiration_audit DROP CONSTRAINT payment_link_expiration_audit_operation_check;
ALTER TABLE fl_billing.payment_link_expiration_audit ADD CONSTRAINT payment_link_expiration_audit_operation_check
 CHECK(operation IN ('reserve','inspect','complete'));
DO $migration$
BEGIN
 EXECUTE format('CREATE POLICY payment_link_expiration_complete_owner ON fl_billing.payment_link_expiration_intents FOR UPDATE TO %I USING(tenant_id=current_setting(''forklaunch.tenant_id'',true)) WITH CHECK(state=''completed'' AND receipt_ciphertext IS NOT NULL)',current_user);
 EXECUTE format('CREATE POLICY payment_link_expiration_claim_release_owner ON fl_billing.payment_link_expiration_dependency_claims FOR DELETE TO %I USING(tenant_id=current_setting(''forklaunch.tenant_id'',true))',current_user);
END;
$migration$;

-- These trigger functions deliberately are INVOKER functions: only the owner
-- context inside the single publisher function may change state or remove claims.
CREATE FUNCTION fl_billing.guard_payment_link_expiration_completion() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $function$
DECLARE owner_oid oid;
BEGIN
 SELECT relowner INTO owner_oid FROM pg_class WHERE oid='fl_billing.payment_link_expiration_dependency_guards'::regclass;
 IF current_user::regrole::oid<>owner_oid OR OLD.state<>'pending' OR NEW.state<>'completed'
  OR NEW.receipt_ciphertext IS NULL OR length(NEW.receipt_ciphertext)<16
  OR (to_jsonb(OLD)-'state'-'receipt_ciphertext') IS DISTINCT FROM (to_jsonb(NEW)-'state'-'receipt_ciphertext') THEN
  RAISE EXCEPTION 'Invalid payment-link expiration completion';
 END IF;
 RETURN NEW;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.guard_payment_link_expiration_completion() FROM PUBLIC;
DROP TRIGGER payment_link_expiration_intent_immutable ON fl_billing.payment_link_expiration_intents;
CREATE TRIGGER payment_link_expiration_intent_completion BEFORE UPDATE ON fl_billing.payment_link_expiration_intents
 FOR EACH ROW EXECUTE FUNCTION fl_billing.guard_payment_link_expiration_completion();
CREATE TRIGGER payment_link_expiration_intent_no_delete BEFORE DELETE OR TRUNCATE ON fl_billing.payment_link_expiration_intents
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.deny_payment_link_expiration_journal_mutation();

CREATE FUNCTION fl_billing.release_completed_payment_link_expiration_claim() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $function$
DECLARE owner_oid oid; changed bigint;
BEGIN
 SELECT relowner INTO owner_oid FROM pg_class WHERE oid='fl_billing.payment_link_expiration_dependency_guards'::regclass;
 IF current_user::regrole::oid<>owner_oid OR NOT EXISTS(SELECT 1 FROM fl_billing.payment_link_expiration_intents
  WHERE tenant_id=OLD.tenant_id AND operation_id=OLD.operation_id AND state='completed' AND receipt_ciphertext IS NOT NULL) THEN
  RAISE EXCEPTION 'Payment-link expiration claim is not releasable';
 END IF;
 UPDATE fl_billing.payment_link_expiration_dependency_guards SET claims=claims-1
 WHERE tenant_id=OLD.tenant_id AND dependency_kind=OLD.dependency_kind AND dependency_id=OLD.dependency_id AND claims>0;
 GET DIAGNOSTICS changed=ROW_COUNT;
 IF changed<>1 THEN RAISE EXCEPTION 'Missing payment-link expiration claim counter'; END IF;
 RETURN OLD;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.release_completed_payment_link_expiration_claim() FROM PUBLIC;
DROP TRIGGER payment_link_expiration_claim_immutable ON fl_billing.payment_link_expiration_dependency_claims;
CREATE TRIGGER payment_link_expiration_claim_no_update BEFORE UPDATE OR TRUNCATE ON fl_billing.payment_link_expiration_dependency_claims
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.deny_payment_link_expiration_journal_mutation();
CREATE FUNCTION fl_billing.guard_payment_link_expiration_release_caller() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $function$
BEGIN
 IF current_user::regrole::oid<>(SELECT relowner FROM pg_class WHERE oid='fl_billing.payment_link_expiration_dependency_guards'::regclass) THEN
  RAISE EXCEPTION 'Payment-link expiration release requires trusted publication';
 END IF;
 RETURN NULL;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.guard_payment_link_expiration_release_caller() FROM PUBLIC;
CREATE TRIGGER payment_link_expiration_claim_delete_caller BEFORE DELETE ON fl_billing.payment_link_expiration_dependency_claims
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.guard_payment_link_expiration_release_caller();
CREATE TRIGGER payment_link_expiration_claim_completed_release BEFORE DELETE ON fl_billing.payment_link_expiration_dependency_claims
 FOR EACH ROW EXECUTE FUNCTION fl_billing.release_completed_payment_link_expiration_claim();

-- Only the explicitly trusted publisher role is granted EXECUTE on this function.
-- Native caller owns the transaction and checks context freshness after this await.
CREATE FUNCTION fl_billing.publish_payment_link_expiration(
 p_tenant text,p_operation uuid,p_reservation uuid,p_hash text,p_expected jsonb,
 p_receipt text,p_correlation text
) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $function$
DECLARE intent fl_billing.payment_link_expiration_intents%ROWTYPE;
 record_data jsonb; record_version text; binding_data jsonb; binding_version text;
 observed jsonb; changed bigint; dep jsonb;
BEGIN
 IF p_tenant IS DISTINCT FROM current_setting('forklaunch.tenant_id',true)
  OR p_tenant IS DISTINCT FROM p_expected->>'tenant'
  OR p_operation::text IS DISTINCT FROM p_expected->>'operation'
  OR p_receipt IS NULL OR length(p_receipt)<16 OR octet_length(p_receipt)>524288
  OR p_correlation IS NULL OR length(btrim(p_correlation))=0 OR length(p_correlation)>256 THEN
  RAISE EXCEPTION 'Invalid payment-link expiration publication';
 END IF;
 dep:=p_expected->'proof'->'observed';
 -- Same deterministic reservation lock order: local record, binding, intent.
 SELECT data,xmin::text INTO record_data,record_version FROM fl_billing.records
 WHERE tenant_id=p_tenant AND scope='durable' AND kind='payment_link'
 AND id=dep->'binding'->>'storage_id' FOR UPDATE;
 SELECT to_jsonb(b),b.xmin::text INTO binding_data,binding_version FROM fl_billing.provider_bindings b
 WHERE tenant_id=p_tenant AND scope='durable' AND kind='payment_link'
 AND binding_id::text=dep->'binding'->>'binding_id' FOR UPDATE;
 IF record_data IS NULL OR binding_data IS NULL OR record_data IS DISTINCT FROM dep->'record'
  OR record_version IS DISTINCT FROM dep->>'recordVersion'
  OR binding_data IS DISTINCT FROM dep->'binding'
  OR binding_version IS DISTINCT FROM dep->>'bindingVersion' THEN
  RAISE EXCEPTION 'Payment-link expiration dependency changed';
 END IF;
 SELECT jsonb_build_object('record',record_data,'recordVersion',record_version,'binding',binding_data,'bindingVersion',binding_version,
  'intent',j.intent_ciphertext,'receipt',j.receipt_ciphertext,'hash',j.payload_hash,'journalVersion',j.xmin::text)
 INTO observed FROM fl_billing.payment_link_creation_intents j
 WHERE tenant_id=p_tenant AND operation_id::text=p_expected->'proof'->>'operation'
 AND public_id::text=p_expected->>'publicId' AND actor_id=p_expected->>'actor' AND state='completed';
 IF observed IS NULL OR observed IS DISTINCT FROM dep THEN
  RAISE EXCEPTION 'Payment-link creation proof changed';
 END IF;
 SELECT * INTO intent FROM fl_billing.payment_link_expiration_intents
 WHERE tenant_id=p_tenant AND operation_id=p_operation FOR UPDATE;
 IF NOT FOUND OR intent.state<>'pending' OR intent.reservation_id<>p_reservation
  OR intent.payload_hash<>p_hash OR intent.actor_id IS DISTINCT FROM p_expected->>'actor'
  OR intent.public_id::text IS DISTINCT FROM p_expected->>'publicId'
  OR intent.deployment_label IS DISTINCT FROM binding_data->>'deployment_label'
  OR intent.account_id IS DISTINCT FROM binding_data->>'account_id'
  OR to_jsonb(intent.livemode) IS DISTINCT FROM binding_data->'livemode'
  OR intent.api_version IS DISTINCT FROM p_expected->>'apiVersion'
  OR binding_data->>'public_id' IS DISTINCT FROM intent.public_id::text
  OR record_data->>'id' IS DISTINCT FROM binding_data->>'storage_id' THEN
  RAISE EXCEPTION 'Payment-link expiration reservation changed';
 END IF;
 IF (SELECT count(*) FROM fl_billing.payment_link_expiration_dependency_claims WHERE tenant_id=p_tenant AND operation_id=p_operation)<>2
  OR NOT EXISTS(SELECT 1 FROM fl_billing.payment_link_expiration_dependency_claims WHERE tenant_id=p_tenant AND operation_id=p_operation AND dependency_kind='payment_link_record' AND dependency_id=binding_data->>'storage_id')
  OR NOT EXISTS(SELECT 1 FROM fl_billing.payment_link_expiration_dependency_claims WHERE tenant_id=p_tenant AND operation_id=p_operation AND dependency_kind='payment_link_binding' AND dependency_id=binding_data->>'binding_id') THEN
  RAISE EXCEPTION 'Missing payment-link expiration claims';
 END IF;
 INSERT INTO fl_billing.payment_link_expiration_audit(id,tenant_id,operation_id,actor_id,correlation_id,operation)
 VALUES(gen_random_uuid(),p_tenant,p_operation,intent.actor_id,p_correlation,'complete');
 UPDATE fl_billing.payment_link_expiration_intents SET state='completed',receipt_ciphertext=p_receipt
 WHERE tenant_id=p_tenant AND operation_id=p_operation;
 DELETE FROM fl_billing.payment_link_expiration_dependency_claims WHERE tenant_id=p_tenant AND operation_id=p_operation;
 GET DIAGNOSTICS changed=ROW_COUNT;
 IF changed<>2 THEN RAISE EXCEPTION 'Incomplete payment-link expiration release'; END IF;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.publish_payment_link_expiration(text,uuid,uuid,text,jsonb,text,text) FROM PUBLIC;

-- Audit history is append-only even if a runtime is accidentally granted TRUNCATE.
CREATE TRIGGER payment_link_expiration_audit_immutable BEFORE UPDATE OR DELETE OR TRUNCATE
 ON fl_billing.payment_link_expiration_audit FOR EACH STATEMENT
 EXECUTE FUNCTION fl_billing.deny_payment_link_expiration_journal_mutation();

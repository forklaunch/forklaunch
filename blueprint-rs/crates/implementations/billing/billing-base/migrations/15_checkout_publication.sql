-- Explicit separate trusted Checkout publisher; ordinary runtime cannot complete/release.
ALTER TABLE fl_billing.checkout_creation_intents
 ADD COLUMN state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','completed')),
 ADD COLUMN receipt_ciphertext text,
 ADD CONSTRAINT checkout_creation_receipt_state CHECK
 ((state='pending' AND receipt_ciphertext IS NULL) OR
  (state='completed' AND receipt_ciphertext IS NOT NULL));
DROP POLICY checkout_creation_insert ON fl_billing.checkout_creation_intents;
CREATE POLICY checkout_creation_insert ON fl_billing.checkout_creation_intents FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true) AND state='pending' AND receipt_ciphertext IS NULL);
ALTER TABLE fl_billing.checkout_creation_audit DROP CONSTRAINT checkout_creation_audit_operation_check;
ALTER TABLE fl_billing.checkout_creation_audit ADD CONSTRAINT checkout_creation_audit_operation_check
 CHECK(operation IN ('reserve','inspect','complete'));
DO $migration$
BEGIN
 EXECUTE format('CREATE POLICY checkout_creation_complete_owner ON fl_billing.checkout_creation_intents FOR UPDATE TO %I USING(tenant_id=current_setting(''forklaunch.tenant_id'',true)) WITH CHECK(state=''completed'' AND receipt_ciphertext IS NOT NULL)',current_user);
 EXECUTE format('CREATE POLICY checkout_claim_release_owner ON fl_billing.checkout_dependency_claims FOR DELETE TO %I USING(tenant_id=current_setting(''forklaunch.tenant_id'',true))',current_user);
END;
$migration$;

-- These trigger functions deliberately are INVOKER functions: only the owner
-- context inside the single publisher function may change state or remove claims.
CREATE FUNCTION fl_billing.guard_checkout_completion() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $function$
DECLARE owner_oid oid;
BEGIN
 SELECT relowner INTO owner_oid FROM pg_class WHERE oid='fl_billing.subscription_dependency_guards'::regclass;
 IF current_user::regrole::oid<>owner_oid OR OLD.state<>'pending' OR NEW.state<>'completed'
  OR NEW.receipt_ciphertext IS NULL OR length(NEW.receipt_ciphertext)<16
  OR (to_jsonb(OLD)-'state'-'receipt_ciphertext') IS DISTINCT FROM (to_jsonb(NEW)-'state'-'receipt_ciphertext') THEN
  RAISE EXCEPTION 'Invalid checkout completion';
 END IF;
 IF NOT EXISTS(SELECT 1 FROM fl_billing.records r JOIN fl_billing.provider_bindings b
  ON b.tenant_id=r.tenant_id AND b.scope=r.scope AND b.kind=r.kind AND b.storage_id=r.id
  WHERE r.tenant_id=NEW.tenant_id AND r.scope='durable' AND r.kind='checkout_session'
  AND r.id=NEW.public_id::text AND r.data->>'id'=r.id AND b.public_id=NEW.public_id
  AND b.deployment_label=NEW.deployment_label
  AND b.account_id=NEW.account_id AND b.livemode=NEW.livemode) THEN
  RAISE EXCEPTION 'Missing checkout publication';
 END IF;
 RETURN NEW;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.guard_checkout_completion() FROM PUBLIC;
DROP TRIGGER checkout_intent_immutable ON fl_billing.checkout_creation_intents;
CREATE TRIGGER checkout_intent_completion BEFORE UPDATE ON fl_billing.checkout_creation_intents
 FOR EACH ROW EXECUTE FUNCTION fl_billing.guard_checkout_completion();
CREATE TRIGGER checkout_intent_no_delete BEFORE DELETE OR TRUNCATE ON fl_billing.checkout_creation_intents
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.deny_checkout_journal_mutation();

CREATE FUNCTION fl_billing.release_completed_checkout_claim() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $function$
DECLARE owner_oid oid; changed bigint;
BEGIN
 SELECT relowner INTO owner_oid FROM pg_class WHERE oid='fl_billing.subscription_dependency_guards'::regclass;
 IF current_user::regrole::oid<>owner_oid OR NOT EXISTS(SELECT 1 FROM fl_billing.checkout_creation_intents
  WHERE tenant_id=OLD.tenant_id AND operation_id=OLD.operation_id AND state='completed' AND receipt_ciphertext IS NOT NULL) THEN
  RAISE EXCEPTION 'Checkout claim is not releasable';
 END IF;
 UPDATE fl_billing.subscription_dependency_guards SET claims=claims-1
 WHERE tenant_id=OLD.tenant_id AND dependency_kind=OLD.dependency_kind AND dependency_id=OLD.dependency_id AND claims>0;
 GET DIAGNOSTICS changed=ROW_COUNT;
 IF changed<>1 THEN RAISE EXCEPTION 'Missing checkout claim counter'; END IF;
 RETURN OLD;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.release_completed_checkout_claim() FROM PUBLIC;
DROP TRIGGER checkout_claim_immutable ON fl_billing.checkout_dependency_claims;
CREATE TRIGGER checkout_claim_no_update BEFORE UPDATE OR TRUNCATE ON fl_billing.checkout_dependency_claims
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.deny_checkout_journal_mutation();
CREATE FUNCTION fl_billing.guard_checkout_release_caller() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $function$
BEGIN
 IF current_user::regrole::oid<>(SELECT relowner FROM pg_class WHERE oid='fl_billing.subscription_dependency_guards'::regclass) THEN
  RAISE EXCEPTION 'Checkout release requires trusted publication';
 END IF;
 RETURN NULL;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.guard_checkout_release_caller() FROM PUBLIC;
CREATE TRIGGER checkout_claim_delete_caller BEFORE DELETE ON fl_billing.checkout_dependency_claims
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.guard_checkout_release_caller();
CREATE TRIGGER checkout_claim_completed_release BEFORE DELETE ON fl_billing.checkout_dependency_claims
 FOR EACH ROW EXECUTE FUNCTION fl_billing.release_completed_checkout_claim();

CREATE FUNCTION fl_billing.publish_bound_checkout(
 p_tenant text,p_operation uuid,p_reservation uuid,p_hash text,p_expected jsonb,
 p_record jsonb,p_provider_id text,p_receipt text,p_correlation text
) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $function$
DECLARE intent fl_billing.checkout_creation_intents%ROWTYPE;
 customer_row jsonb; plan_row jsonb; customer_version text; plan_version text;
 customer_binding jsonb; plan_binding jsonb; dependency jsonb;
 plan_intent text; plan_receipt text; new_binding_id uuid; changed bigint;
BEGIN
 IF p_tenant IS DISTINCT FROM current_setting('forklaunch.tenant_id',true)
  OR p_tenant IS DISTINCT FROM p_expected->>'tenant'
  OR p_operation::text IS DISTINCT FROM p_expected->>'operation'
  OR p_record IS NULL OR p_receipt IS NULL OR length(p_receipt)<16 OR length(p_receipt)>2097152
  OR p_correlation IS NULL OR length(btrim(p_correlation))=0 OR length(p_correlation)>256 THEN
  RAISE EXCEPTION 'Invalid checkout publication';
 END IF;
 dependency:=p_expected->'dependencies';
 -- Same lock order as snapshot/reservation: two records, then two bindings.
 SELECT data,xmin::text INTO customer_row,customer_version FROM fl_billing.records
 WHERE tenant_id=p_tenant AND scope='durable' AND kind='customer' AND id=p_expected->>'customer' FOR UPDATE;
 SELECT data,xmin::text INTO plan_row,plan_version FROM fl_billing.records
 WHERE tenant_id=p_tenant AND scope='durable' AND kind='plan' AND id=p_expected->>'plan' FOR UPDATE;
 SELECT to_jsonb(b) INTO customer_binding FROM fl_billing.stripe_customer_bindings b
 WHERE tenant_id=p_tenant AND binding_id::text=dependency->'customerBinding'->>'binding_id' FOR UPDATE;
 SELECT to_jsonb(b) INTO plan_binding FROM fl_billing.provider_bindings b
 WHERE tenant_id=p_tenant AND kind='plan' AND binding_id::text=dependency->'planBinding'->>'binding_id' FOR UPDATE;
 IF customer_row IS NULL OR plan_row IS NULL OR customer_binding IS NULL OR plan_binding IS NULL
  OR customer_row IS DISTINCT FROM dependency->'customerData' OR plan_row IS DISTINCT FROM dependency->'planData'
  OR customer_version IS DISTINCT FROM dependency->>'customerVersion' OR plan_version IS DISTINCT FROM dependency->>'planVersion'
  OR customer_binding IS DISTINCT FROM dependency->'customerBinding' OR plan_binding IS DISTINCT FROM dependency->'planBinding' THEN
  RAISE EXCEPTION 'Checkout dependencies changed';
 END IF;
 SELECT * INTO intent FROM fl_billing.checkout_creation_intents
 WHERE tenant_id=p_tenant AND operation_id=p_operation FOR UPDATE;
 IF NOT FOUND OR intent.state<>'pending' OR intent.reservation_id<>p_reservation OR intent.payload_hash<>p_hash
  OR intent.actor_id IS DISTINCT FROM p_expected->>'actor' OR intent.deployment_label IS DISTINCT FROM p_expected->>'deployment'
  OR intent.account_id IS DISTINCT FROM p_expected->>'account' OR to_jsonb(intent.livemode) IS DISTINCT FROM p_expected->'livemode'
  OR intent.api_version IS DISTINCT FROM p_expected->>'apiVersion' THEN
  RAISE EXCEPTION 'Checkout reservation changed';
 END IF;
 IF customer_row->>'id' IS DISTINCT FROM p_expected->>'customer'
  OR plan_row->>'id' IS DISTINCT FROM p_expected->>'plan'
  OR customer_binding->>'local_customer_id' IS DISTINCT FROM p_expected->>'customer'
  OR customer_binding->>'storage_id' IS DISTINCT FROM p_expected->>'customer'
  OR plan_binding->>'public_id' IS DISTINCT FROM p_expected->>'plan'
  OR plan_binding->>'storage_id' IS DISTINCT FROM p_expected->>'plan'
  OR customer_binding->>'deployment_label' IS DISTINCT FROM intent.deployment_label
  OR plan_binding->>'deployment_label' IS DISTINCT FROM intent.deployment_label
  OR customer_binding->>'account_id' IS DISTINCT FROM intent.account_id
  OR plan_binding->>'account_id' IS DISTINCT FROM intent.account_id
  OR customer_binding->'livemode' IS DISTINCT FROM to_jsonb(intent.livemode)
  OR plan_binding->'livemode' IS DISTINCT FROM to_jsonb(intent.livemode)
  OR customer_binding->>'verified_api_version' IS DISTINCT FROM intent.api_version THEN
  RAISE EXCEPTION 'Checkout relation changed';
 END IF;
 SELECT intent_ciphertext,receipt_ciphertext INTO plan_intent,plan_receipt FROM fl_billing.plan_creation_intents
 WHERE tenant_id=p_tenant AND operation_id::text=p_expected->>'planOperation' AND public_id::text=p_expected->>'plan'
 AND deployment_label=intent.deployment_label AND account_id=intent.account_id AND livemode=intent.livemode
 AND api_version=intent.api_version AND state='completed';
 IF NOT FOUND OR plan_intent IS DISTINCT FROM dependency->>'planIntentCiphertext'
  OR plan_receipt IS DISTINCT FROM dependency->>'planReceiptCiphertext' THEN
  RAISE EXCEPTION 'Plan creation provenance changed';
 END IF;
 IF p_record->>'id' IS DISTINCT FROM intent.public_id::text
  OR p_record->>'customerId' IS DISTINCT FROM p_expected->>'customer'
  OR p_record->>'status' IS DISTINCT FROM 'PENDING'
  OR p_record->'paymentMethods' IS DISTINCT FROM '["card"]'::jsonb
  OR jsonb_typeof(p_record->'providerFields') IS DISTINCT FROM 'string'
  OR jsonb_typeof(p_record->'uri') IS DISTINCT FROM 'string'
  OR jsonb_typeof(p_record->'expiresAt') IS DISTINCT FROM 'string'
  OR p_provider_id IS NULL OR length(btrim(p_provider_id))=0 OR octet_length(p_provider_id)>1024
  THEN RAISE EXCEPTION 'Invalid checkout output'; END IF;
 IF (SELECT count(*) FROM fl_billing.checkout_dependency_claims WHERE tenant_id=p_tenant AND operation_id=p_operation)<>4
  OR NOT EXISTS(SELECT 1 FROM fl_billing.checkout_dependency_claims WHERE tenant_id=p_tenant AND operation_id=p_operation AND dependency_kind='customer_record' AND dependency_id=p_expected->>'customer')
  OR NOT EXISTS(SELECT 1 FROM fl_billing.checkout_dependency_claims WHERE tenant_id=p_tenant AND operation_id=p_operation AND dependency_kind='plan_record' AND dependency_id=p_expected->>'plan')
  OR NOT EXISTS(SELECT 1 FROM fl_billing.checkout_dependency_claims WHERE tenant_id=p_tenant AND operation_id=p_operation AND dependency_kind='customer_binding' AND dependency_id=customer_binding->>'binding_id')
  OR NOT EXISTS(SELECT 1 FROM fl_billing.checkout_dependency_claims WHERE tenant_id=p_tenant AND operation_id=p_operation AND dependency_kind='plan_binding' AND dependency_id=plan_binding->>'binding_id') THEN
  RAISE EXCEPTION 'Missing checkout dependency claims';
 END IF;
 new_binding_id:=gen_random_uuid();
 INSERT INTO fl_billing.records(tenant_id,scope,kind,id,data) VALUES(p_tenant,'durable','checkout_session',intent.public_id::text,p_record);
 INSERT INTO fl_billing.provider_bindings(tenant_id,kind,public_id,binding_id,storage_id,deployment_label,account_id,livemode,provider_id)
 VALUES(p_tenant,'checkout_session',intent.public_id,new_binding_id,intent.public_id::text,intent.deployment_label,intent.account_id,intent.livemode,p_provider_id);
 INSERT INTO fl_billing.provider_binding_audit(id,tenant_id,binding_id,public_id,kind,storage_id,deployment_label,account_id,livemode,provider_id,operation,actor_id,correlation_id)
 VALUES(gen_random_uuid(),p_tenant,new_binding_id,intent.public_id,'checkout_session',intent.public_id::text,intent.deployment_label,intent.account_id,intent.livemode,p_provider_id,'provision',intent.actor_id,p_correlation);
 INSERT INTO fl_billing.checkout_creation_audit(id,tenant_id,operation_id,actor_id,correlation_id,operation)
 VALUES(gen_random_uuid(),p_tenant,p_operation,intent.actor_id,p_correlation,'complete');
 UPDATE fl_billing.checkout_creation_intents SET state='completed',receipt_ciphertext=p_receipt
 WHERE tenant_id=p_tenant AND operation_id=p_operation;
 DELETE FROM fl_billing.checkout_dependency_claims WHERE tenant_id=p_tenant AND operation_id=p_operation;
 GET DIAGNOSTICS changed=ROW_COUNT;
 IF changed<>4 THEN RAISE EXCEPTION 'Incomplete checkout claim release'; END IF;
 -- No COMMIT: native caller rechecks freshness after this await, then commits
 -- this same transaction. Any error rolls back record/binding/receipt/counters.
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.publish_bound_checkout(text,uuid,uuid,text,jsonb,jsonb,text,text,text) FROM PUBLIC;

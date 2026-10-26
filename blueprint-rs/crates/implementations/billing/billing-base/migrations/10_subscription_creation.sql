-- Stage A only: immutable reservations and durable dependency barriers.
-- No dispatch, completion, release, TTL expiry or financial operation is exposed.
CREATE TABLE fl_billing.subscription_creation_intents (
 tenant_id text NOT NULL, operation_id uuid NOT NULL, public_id uuid NOT NULL,
 reservation_id uuid NOT NULL, actor_id text NOT NULL,
 deployment_label text NOT NULL, account_id text NOT NULL, livemode boolean NOT NULL,
 api_version text NOT NULL, payload_hash text NOT NULL, idempotency_key text NOT NULL,
 intent_ciphertext text NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(tenant_id,operation_id), UNIQUE(tenant_id,public_id), UNIQUE(idempotency_key)
);
ALTER TABLE fl_billing.subscription_creation_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.subscription_creation_intents FORCE ROW LEVEL SECURITY;
CREATE POLICY subscription_creation_read ON fl_billing.subscription_creation_intents FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY subscription_creation_insert ON fl_billing.subscription_creation_intents FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

CREATE TABLE fl_billing.subscription_dependency_claims (
 tenant_id text NOT NULL, operation_id uuid NOT NULL,
 dependency_kind text NOT NULL CHECK(dependency_kind IN
 ('customer_record','plan_record','customer_binding','plan_binding')),
 dependency_id text NOT NULL,
 PRIMARY KEY(tenant_id,operation_id,dependency_kind),
 FOREIGN KEY(tenant_id,operation_id)
 REFERENCES fl_billing.subscription_creation_intents(tenant_id,operation_id) ON DELETE RESTRICT
);
ALTER TABLE fl_billing.subscription_dependency_claims ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.subscription_dependency_claims FORCE ROW LEVEL SECURITY;
CREATE POLICY subscription_dependency_read ON fl_billing.subscription_dependency_claims FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY subscription_dependency_insert ON fl_billing.subscription_dependency_claims FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

-- Private counters prevent a stale SQL statement snapshot from overlooking a
-- reservation that committed while its mutation waited for the authoritative row.
-- UPDATE RETURNING observes the locked current counter, unlike a plain SELECT.
CREATE TABLE fl_billing.subscription_dependency_guards (
 tenant_id text NOT NULL, dependency_kind text NOT NULL, dependency_id text NOT NULL,
 claims bigint NOT NULL CHECK(claims>=0),
 PRIMARY KEY(tenant_id,dependency_kind,dependency_id)
);
ALTER TABLE fl_billing.subscription_dependency_guards ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.subscription_dependency_guards FORCE ROW LEVEL SECURITY;
DO $migration$
BEGIN
 EXECUTE format('CREATE POLICY subscription_guard_owner ON fl_billing.subscription_dependency_guards TO %I USING(true) WITH CHECK(true)',current_user);
END;
$migration$;
REVOKE ALL ON fl_billing.subscription_dependency_guards FROM PUBLIC;
-- Runtime MUST be a distinct non-inheriting role, not a member of migration owner.
-- Even broad runtime table grants do not receive the private owner RLS policy.

-- Guard rows exist before any claim, including for unclaimed dependencies.
-- Thus a waiting UPDATE sees a changed counter through PostgreSQL row locking
-- (or a serialization failure at stronger isolation), never an invisible insert.
DO $migration$
DECLARE target text;
BEGIN
 FOREACH target IN ARRAY ARRAY['records','stripe_customer_bindings','provider_bindings'] LOOP
  EXECUTE format('CREATE POLICY subscription_migration_read ON fl_billing.%I FOR SELECT TO %I USING(true)',target,current_user);
 END LOOP;
END;
$migration$;
INSERT INTO fl_billing.subscription_dependency_guards
 SELECT tenant_id,kind||'_record',id,0 FROM fl_billing.records
 WHERE scope='durable' AND kind IN ('customer','plan');
INSERT INTO fl_billing.subscription_dependency_guards
 SELECT tenant_id,'customer_binding',binding_id::text,0 FROM fl_billing.stripe_customer_bindings;
INSERT INTO fl_billing.subscription_dependency_guards
 SELECT tenant_id,'plan_binding',binding_id::text,0 FROM fl_billing.provider_bindings WHERE kind='plan';
DROP POLICY subscription_migration_read ON fl_billing.records;
DROP POLICY subscription_migration_read ON fl_billing.stripe_customer_bindings;
DROP POLICY subscription_migration_read ON fl_billing.provider_bindings;
CREATE FUNCTION fl_billing.initialize_subscription_guard() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $function$
DECLARE target_kind text; target_id text;
BEGIN
 IF TG_TABLE_NAME='records' THEN
  IF NEW.scope<>'durable' OR NEW.kind NOT IN ('customer','plan') THEN RETURN NEW; END IF;
  target_kind:=NEW.kind||'_record'; target_id:=NEW.id;
 ELSIF TG_TABLE_NAME='stripe_customer_bindings' THEN
  target_kind:='customer_binding'; target_id:=NEW.binding_id::text;
 ELSE
  IF NEW.kind<>'plan' THEN RETURN NEW; END IF;
  target_kind:='plan_binding'; target_id:=NEW.binding_id::text;
 END IF;
 INSERT INTO fl_billing.subscription_dependency_guards(tenant_id,dependency_kind,dependency_id,claims)
 VALUES(NEW.tenant_id,target_kind,target_id,0) ON CONFLICT DO NOTHING;
 RETURN NEW;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.initialize_subscription_guard() FROM PUBLIC;
CREATE TRIGGER subscription_record_initialize AFTER INSERT OR UPDATE ON fl_billing.records
 FOR EACH ROW EXECUTE FUNCTION fl_billing.initialize_subscription_guard();
CREATE TRIGGER subscription_customer_initialize AFTER INSERT OR UPDATE ON fl_billing.stripe_customer_bindings
 FOR EACH ROW EXECUTE FUNCTION fl_billing.initialize_subscription_guard();
CREATE TRIGGER subscription_plan_initialize AFTER INSERT OR UPDATE ON fl_billing.provider_bindings
 FOR EACH ROW EXECUTE FUNCTION fl_billing.initialize_subscription_guard();

CREATE FUNCTION fl_billing.claim_subscription_dependency() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $function$
DECLARE found_id text;
BEGIN
 IF NEW.tenant_id IS DISTINCT FROM current_setting('forklaunch.tenant_id',true) THEN
  RAISE EXCEPTION 'Invalid subscription dependency tenant';
 END IF;
 IF NEW.dependency_kind IN ('customer_record','plan_record') THEN
  SELECT id INTO found_id FROM fl_billing.records
   WHERE tenant_id=NEW.tenant_id AND scope='durable'
   AND kind=replace(NEW.dependency_kind,'_record','') AND id=NEW.dependency_id FOR UPDATE;
 ELSIF NEW.dependency_kind='customer_binding' THEN
  SELECT binding_id::text INTO found_id FROM fl_billing.stripe_customer_bindings
   WHERE tenant_id=NEW.tenant_id AND binding_id::text=NEW.dependency_id FOR UPDATE;
 ELSIF NEW.dependency_kind='plan_binding' THEN
  SELECT binding_id::text INTO found_id FROM fl_billing.provider_bindings
   WHERE tenant_id=NEW.tenant_id AND kind='plan' AND binding_id::text=NEW.dependency_id FOR UPDATE;
 END IF;
 IF found_id IS NULL THEN RAISE EXCEPTION 'Missing subscription dependency'; END IF;
 INSERT INTO fl_billing.subscription_dependency_guards(tenant_id,dependency_kind,dependency_id,claims)
 VALUES(NEW.tenant_id,NEW.dependency_kind,NEW.dependency_id,1)
 ON CONFLICT(tenant_id,dependency_kind,dependency_id)
 DO UPDATE SET claims=fl_billing.subscription_dependency_guards.claims+1;
 RETURN NEW;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.claim_subscription_dependency() FROM PUBLIC;
CREATE TRIGGER subscription_dependency_claim AFTER INSERT ON fl_billing.subscription_dependency_claims
 FOR EACH ROW EXECUTE FUNCTION fl_billing.claim_subscription_dependency();

CREATE FUNCTION fl_billing.guard_subscription_dependency() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $function$
DECLARE target_kind text; target_id text; held bigint;
BEGIN
 IF TG_TABLE_NAME='records' THEN
  IF OLD.scope<>'durable' OR OLD.kind NOT IN ('customer','plan') THEN
   IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
  END IF;
  target_kind:=OLD.kind||'_record'; target_id:=OLD.id;
 ELSIF TG_TABLE_NAME='stripe_customer_bindings' THEN
  target_kind:='customer_binding'; target_id:=OLD.binding_id::text;
 ELSE
  IF OLD.kind<>'plan' THEN
   IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
  END IF;
  target_kind:='plan_binding'; target_id:=OLD.binding_id::text;
 END IF;
 UPDATE fl_billing.subscription_dependency_guards g SET claims=g.claims
  WHERE g.tenant_id=OLD.tenant_id AND g.dependency_kind=target_kind
   AND g.dependency_id=target_id RETURNING g.claims INTO held;
 IF held IS NULL OR held>0 THEN RAISE EXCEPTION 'Subscription dependency is claimed'; END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.guard_subscription_dependency() FROM PUBLIC;
CREATE TRIGGER subscription_record_barrier BEFORE UPDATE OR DELETE ON fl_billing.records
 FOR EACH ROW EXECUTE FUNCTION fl_billing.guard_subscription_dependency();
CREATE TRIGGER subscription_customer_barrier BEFORE UPDATE OR DELETE ON fl_billing.stripe_customer_bindings
 FOR EACH ROW EXECUTE FUNCTION fl_billing.guard_subscription_dependency();
CREATE TRIGGER subscription_plan_barrier BEFORE UPDATE OR DELETE ON fl_billing.provider_bindings
 FOR EACH ROW EXECUTE FUNCTION fl_billing.guard_subscription_dependency();

CREATE FUNCTION fl_billing.deny_subscription_journal_mutation() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog AS $function$
BEGIN RAISE EXCEPTION 'Subscription reservation is immutable; no release is implemented'; END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.deny_subscription_journal_mutation() FROM PUBLIC;
CREATE TRIGGER subscription_intent_immutable BEFORE UPDATE OR DELETE OR TRUNCATE
 ON fl_billing.subscription_creation_intents FOR EACH STATEMENT
 EXECUTE FUNCTION fl_billing.deny_subscription_journal_mutation();
CREATE TRIGGER subscription_guard_no_truncate BEFORE TRUNCATE
 ON fl_billing.subscription_dependency_guards FOR EACH STATEMENT
 EXECUTE FUNCTION fl_billing.deny_subscription_journal_mutation();
CREATE TRIGGER subscription_claim_immutable BEFORE UPDATE OR DELETE OR TRUNCATE
 ON fl_billing.subscription_dependency_claims FOR EACH STATEMENT
 EXECUTE FUNCTION fl_billing.deny_subscription_journal_mutation();
CREATE FUNCTION fl_billing.guard_subscription_truncate() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $function$
BEGIN
 IF EXISTS(SELECT 1 FROM fl_billing.subscription_dependency_guards WHERE claims>0) THEN
  RAISE EXCEPTION 'Subscription dependencies are claimed';
 END IF;
 RETURN NULL;
END;
$function$;
REVOKE ALL ON FUNCTION fl_billing.guard_subscription_truncate() FROM PUBLIC;
CREATE TRIGGER subscription_record_truncate BEFORE TRUNCATE ON fl_billing.records
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.guard_subscription_truncate();
CREATE TRIGGER subscription_customer_truncate BEFORE TRUNCATE ON fl_billing.stripe_customer_bindings
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.guard_subscription_truncate();
CREATE TRIGGER subscription_plan_truncate BEFORE TRUNCATE ON fl_billing.provider_bindings
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.guard_subscription_truncate();

CREATE TABLE fl_billing.subscription_creation_audit (
 id uuid PRIMARY KEY, tenant_id text NOT NULL, operation_id uuid NOT NULL,
 actor_id text NOT NULL, correlation_id text NOT NULL,
 operation text NOT NULL CHECK(operation IN ('reserve','inspect')),
 created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_billing.subscription_creation_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.subscription_creation_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY subscription_audit_read ON fl_billing.subscription_creation_audit FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY subscription_audit_insert ON fl_billing.subscription_creation_audit FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

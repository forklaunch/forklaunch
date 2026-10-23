-- Local access revocation only. No provider URL invalidation or financial effect.
CREATE TABLE fl_billing.portal_local_expirations (
 tenant_id text NOT NULL,
 public_id uuid NOT NULL,
 operation_id uuid NOT NULL,
 actor_id text NOT NULL,
 profile_hash text NOT NULL,
 binding_id uuid NOT NULL,
 correlation_id text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(tenant_id,public_id),
 FOREIGN KEY(tenant_id,operation_id) REFERENCES fl_billing.portal_creation_intents(tenant_id,operation_id)
);
ALTER TABLE fl_billing.portal_local_expirations ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.portal_local_expirations FORCE ROW LEVEL SECURITY;
CREATE POLICY portal_expiration_read ON fl_billing.portal_local_expirations FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY portal_expiration_append ON fl_billing.portal_local_expirations FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE FUNCTION fl_billing.guard_portal_expiration_history() RETURNS trigger
 LANGUAGE plpgsql AS $$ BEGIN
 RAISE EXCEPTION 'Portal expiration history is immutable';
 END $$;
REVOKE ALL ON FUNCTION fl_billing.guard_portal_expiration_history() FROM PUBLIC;
CREATE TRIGGER portal_expiration_immutable BEFORE UPDATE OR DELETE ON fl_billing.portal_local_expirations
 FOR EACH ROW EXECUTE FUNCTION fl_billing.guard_portal_expiration_history();
CREATE TRIGGER portal_expiration_no_truncate BEFORE TRUNCATE ON fl_billing.portal_local_expirations
 FOR EACH STATEMENT EXECUTE FUNCTION fl_billing.guard_portal_expiration_history();

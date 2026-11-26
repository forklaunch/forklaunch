-- Explicit trusted billing-party associations verified against provider Customers.
-- No FK to a Party: users/organizations may be owned by another service/database.
-- The authorized server caller asserts that external association/existence.
CREATE TABLE fl_billing.stripe_customer_bindings (
 tenant_id text NOT NULL,
 deployment_label text NOT NULL,
 account_id text NOT NULL,
 livemode boolean NOT NULL,
 local_customer_id uuid NOT NULL,
 binding_id uuid NOT NULL,
 storage_id text NOT NULL CHECK(storage_id=local_customer_id::text),
 scope text NOT NULL DEFAULT 'durable' CHECK(scope='durable'),
 kind text NOT NULL DEFAULT 'customer' CHECK(kind='customer'),
 provider_customer_id text NOT NULL,
 party_type text NOT NULL,
 party_id text NOT NULL,
 verified_api_version text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(tenant_id,deployment_label,account_id,livemode,local_customer_id),
 UNIQUE(tenant_id,binding_id),
 UNIQUE(tenant_id,deployment_label,account_id,livemode,provider_customer_id),
 UNIQUE(tenant_id,deployment_label,account_id,livemode,party_type,party_id),
 FOREIGN KEY(tenant_id,scope,kind,storage_id) REFERENCES fl_billing.records(tenant_id,scope,kind,id) ON DELETE RESTRICT
);
ALTER TABLE fl_billing.stripe_customer_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.stripe_customer_bindings FORCE ROW LEVEL SECURITY;
CREATE POLICY stripe_customer_binding_tenant ON fl_billing.stripe_customer_bindings
 USING(tenant_id=current_setting('forklaunch.tenant_id',true))
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE FUNCTION fl_billing.immutable_stripe_customer_binding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'Customer binding is immutable; revoke its exact generation first'; END;
$$;
CREATE TRIGGER stripe_customer_binding_immutable BEFORE UPDATE ON fl_billing.stripe_customer_bindings
 FOR EACH ROW EXECUTE FUNCTION fl_billing.immutable_stripe_customer_binding();
CREATE TABLE fl_billing.stripe_customer_binding_audit (
 id uuid PRIMARY KEY,
 tenant_id text NOT NULL,
 binding_id uuid NOT NULL,
 local_customer_id uuid NOT NULL,
 deployment_label text NOT NULL,
 account_id text NOT NULL,
 livemode boolean NOT NULL,
 provider_customer_id text NOT NULL,
 party_type text NOT NULL,
 party_id text NOT NULL,
 actor_id text NOT NULL,
 correlation_id text NOT NULL,
 operation text NOT NULL CHECK(operation IN ('register','verify','revoke')),
 created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_billing.stripe_customer_binding_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_billing.stripe_customer_binding_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY stripe_customer_audit_read ON fl_billing.stripe_customer_binding_audit FOR SELECT
 USING(tenant_id=current_setting('forklaunch.tenant_id',true));
CREATE POLICY stripe_customer_audit_append ON fl_billing.stripe_customer_binding_audit FOR INSERT
 WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));
-- No Subscription dispatch is exposed by this migration. A future dispatcher
-- MUST add persistent dependency claims/barriers around local relation/Plan revoke
-- or deletion. This FK does not synchronize external IAM-party lifecycle changes.

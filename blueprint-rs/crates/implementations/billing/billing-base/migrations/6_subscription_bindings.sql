-- Administrative ownership assertions only; no provider verification or backfill.
-- Existing alias generations, record keys and ciphertext remain unchanged.
ALTER TABLE fl_billing.provider_bindings DROP CONSTRAINT provider_bindings_kind_check;
ALTER TABLE fl_billing.provider_bindings ADD CONSTRAINT provider_bindings_kind_check
 CHECK(kind IN ('checkout_session','payment_link','billing_portal','subscription'));
ALTER TABLE fl_billing.provider_bindings
 ADD COLUMN subscription_provider_customer_id text,
 ADD COLUMN local_party_id text,
 ADD COLUMN local_party_type text;
ALTER TABLE fl_billing.provider_bindings ADD CONSTRAINT provider_bindings_subscription_relation_check
 CHECK (
 (kind = 'subscription' AND provider_id ~ '^sub_[A-Za-z0-9_]+$' AND length(provider_id) <= 128 AND subscription_provider_customer_id IS NOT NULL AND subscription_provider_customer_id ~ '^cus_[A-Za-z0-9_]+$' AND length(subscription_provider_customer_id) <= 128
  AND local_party_id IS NOT NULL AND length(btrim(local_party_id)) > 0
  AND local_party_type IS NOT NULL AND length(btrim(local_party_type)) > 0)
 OR (kind <> 'subscription' AND subscription_provider_customer_id IS NULL AND local_party_id IS NULL AND local_party_type IS NULL)
);
ALTER TABLE fl_billing.provider_binding_audit
 ADD COLUMN subscription_provider_customer_id text,
 ADD COLUMN local_party_id text,
 ADD COLUMN local_party_type text;
ALTER TABLE fl_billing.provider_binding_audit ADD CONSTRAINT provider_binding_audit_subscription_relation_check
 CHECK (
 (kind = 'subscription' AND provider_id ~ '^sub_[A-Za-z0-9_]+$' AND length(provider_id) <= 128 AND subscription_provider_customer_id IS NOT NULL AND subscription_provider_customer_id ~ '^cus_[A-Za-z0-9_]+$' AND length(subscription_provider_customer_id) <= 128
  AND local_party_id IS NOT NULL AND length(btrim(local_party_id)) > 0
  AND local_party_type IS NOT NULL AND length(btrim(local_party_type)) > 0)
 OR (kind <> 'subscription' AND subscription_provider_customer_id IS NULL AND local_party_id IS NULL AND local_party_type IS NULL)
);

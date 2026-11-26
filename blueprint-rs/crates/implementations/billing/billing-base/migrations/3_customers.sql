-- Extend the existing durable record store without rewriting prior migrations.
ALTER TABLE fl_billing.records DROP CONSTRAINT records_kind_check;
ALTER TABLE fl_billing.records ADD CONSTRAINT records_kind_check
    CHECK(kind IN ('customer','plan','subscription','checkout_session','payment_link','billing_portal'));
ALTER TABLE fl_billing.records ADD CONSTRAINT billing_customer_durable
    CHECK(kind <> 'customer' OR scope = 'durable');

-- Provider identities are explicitly provisioned; email claims never link accounts.
CREATE TABLE fl_iam_provider_identities (
    tenant_id TEXT NOT NULL,
    issuer TEXT NOT NULL CHECK (octet_length(issuer) BETWEEN 1 AND 1024),
    subject TEXT NOT NULL CHECK (octet_length(subject) BETWEEN 1 AND 1024),
    user_id TEXT NOT NULL,
    PRIMARY KEY (tenant_id, issuer, subject),
    FOREIGN KEY (tenant_id, user_id) REFERENCES fl_iam_users(tenant_id, id) ON DELETE CASCADE
);
CREATE INDEX fl_iam_provider_identities_user ON fl_iam_provider_identities(tenant_id,user_id);
ALTER TABLE fl_iam_provider_identities ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_provider_identities FORCE ROW LEVEL SECURITY;
CREATE POLICY fl_iam_provider_identity_tenant ON fl_iam_provider_identities
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));

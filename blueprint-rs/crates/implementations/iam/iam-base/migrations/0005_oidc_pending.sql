CREATE TABLE fl_iam_oidc_pending (
    tenant_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    browser_hash TEXT NOT NULL,
    state_hash TEXT NOT NULL,
    encrypted_pending TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, namespace, browser_hash),
    FOREIGN KEY (tenant_id,namespace,browser_hash) REFERENCES fl_iam_browser_sessions(tenant_id,namespace,id_hash) ON DELETE CASCADE
);
CREATE INDEX fl_iam_oidc_pending_expiry ON fl_iam_oidc_pending(tenant_id,namespace,expires_at);
ALTER TABLE fl_iam_oidc_pending ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_oidc_pending FORCE ROW LEVEL SECURITY;
CREATE POLICY fl_iam_oidc_pending_tenant ON fl_iam_oidc_pending
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));

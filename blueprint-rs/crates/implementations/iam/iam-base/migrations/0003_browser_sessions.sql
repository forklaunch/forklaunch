CREATE TABLE fl_iam_browser_sessions (
    tenant_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    id_hash TEXT NOT NULL,
    encrypted_record TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, namespace, id_hash)
);
CREATE INDEX fl_iam_browser_sessions_expiry ON fl_iam_browser_sessions (tenant_id, namespace, expires_at);
ALTER TABLE fl_iam_browser_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_browser_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY forklaunch_tenant_policy ON fl_iam_browser_sessions
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));

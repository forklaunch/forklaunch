CREATE TABLE fl_iam_users (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    email TEXT NOT NULL,
    user_json JSONB NOT NULL,
    password_hash TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, email)
);
CREATE TABLE fl_iam_organizations (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    slug TEXT NOT NULL,
    organization_json JSONB NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, slug)
);
CREATE TABLE fl_iam_roles (
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    role_json JSONB NOT NULL,
    PRIMARY KEY (tenant_id, name)
);
CREATE TABLE fl_iam_sessions (
    tenant_id TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    user_id TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, token_hash),
    FOREIGN KEY (tenant_id, user_id) REFERENCES fl_iam_users(tenant_id, id) ON DELETE CASCADE
);
CREATE INDEX fl_iam_sessions_expiry ON fl_iam_sessions (expires_at);

ALTER TABLE fl_iam_users ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_users FORCE ROW LEVEL SECURITY;
CREATE POLICY forklaunch_tenant_policy ON fl_iam_users
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));
ALTER TABLE fl_iam_organizations ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_organizations FORCE ROW LEVEL SECURITY;
CREATE POLICY forklaunch_tenant_policy ON fl_iam_organizations
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));
ALTER TABLE fl_iam_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_roles FORCE ROW LEVEL SECURITY;
CREATE POLICY forklaunch_tenant_policy ON fl_iam_roles
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));
ALTER TABLE fl_iam_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY forklaunch_tenant_policy ON fl_iam_sessions
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));

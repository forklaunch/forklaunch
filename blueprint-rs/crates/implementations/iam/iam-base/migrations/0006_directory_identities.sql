-- Administrative links only: matching email/provider claims never create links.
CREATE TABLE fl_iam_directory_identities (
    tenant_id TEXT NOT NULL,
    directory_kind TEXT NOT NULL DEFAULT 'user' CHECK (directory_kind = 'user'),
    directory_user_id TEXT NOT NULL,
    auth_user_id TEXT NOT NULL,
    PRIMARY KEY (tenant_id, directory_user_id),
    UNIQUE (tenant_id, auth_user_id),
    FOREIGN KEY (tenant_id, directory_kind, directory_user_id)
        REFERENCES fl_iam_directory(tenant_id, kind, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, auth_user_id)
        REFERENCES fl_iam_users(tenant_id, id) ON DELETE CASCADE
);
ALTER TABLE fl_iam_directory_identities ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_directory_identities FORCE ROW LEVEL SECURITY;
CREATE POLICY fl_iam_directory_identity_tenant ON fl_iam_directory_identities
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));

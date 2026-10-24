-- Run with the migration role in this service's own database.
CREATE TABLE records (
    tenant_id text NOT NULL,
    id uuid NOT NULL,
    name_ciphertext text NOT NULL,
    PRIMARY KEY (tenant_id, id)
);
ALTER TABLE records ENABLE ROW LEVEL SECURITY;
ALTER TABLE records FORCE ROW LEVEL SECURITY;
CREATE POLICY forklaunch_tenant_policy ON records
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));
-- Grant SELECT/INSERT/UPDATE/DELETE to the configured restricted runtime role
-- through deployment provisioning; no superuser credentials belong in DATABASE_URL.

CREATE TABLE fl_iam_directory (
    tenant_id text NOT NULL,
    kind text NOT NULL CHECK(kind IN ('permission','role','user','organization')),
    id text NOT NULL,
    data jsonb NOT NULL CHECK(jsonb_typeof(data)='object'),
    sequence bigint GENERATED ALWAYS AS IDENTITY,
    PRIMARY KEY(tenant_id,kind,id),
    CHECK(data->>'id'=id)
);
CREATE INDEX fl_iam_directory_membership ON fl_iam_directory(tenant_id,(data->>'organization')) WHERE kind='user';
CREATE INDEX fl_iam_directory_relationships ON fl_iam_directory USING gin(data jsonb_path_ops);
CREATE TABLE fl_iam_directory_unique_claims (
    tenant_id text NOT NULL,
    kind text NOT NULL,
    id text NOT NULL,
    field text NOT NULL,
    blind_index text NOT NULL,
    PRIMARY KEY(tenant_id,kind,field,blind_index),
    FOREIGN KEY(tenant_id,kind,id) REFERENCES fl_iam_directory(tenant_id,kind,id) ON DELETE CASCADE
);
CREATE TABLE fl_iam_directory_audit (
    id uuid PRIMARY KEY,
    tenant_id text NOT NULL,
    kind text NOT NULL,
    record_id text NOT NULL,
    actor_id text,
    correlation_id text NOT NULL,
    operation text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE fl_iam_directory ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_directory FORCE ROW LEVEL SECURITY;
CREATE POLICY fl_iam_directory_tenant ON fl_iam_directory
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));
ALTER TABLE fl_iam_directory_unique_claims ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_directory_unique_claims FORCE ROW LEVEL SECURITY;
CREATE POLICY fl_iam_directory_claims_tenant ON fl_iam_directory_unique_claims
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));
ALTER TABLE fl_iam_directory_audit ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_iam_directory_audit FORCE ROW LEVEL SECURITY;
CREATE POLICY fl_iam_directory_audit_tenant ON fl_iam_directory_audit
    USING(tenant_id=current_setting('forklaunch.tenant_id',true))
    WITH CHECK(tenant_id=current_setting('forklaunch.tenant_id',true));

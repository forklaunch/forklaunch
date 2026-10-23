CREATE TABLE fl_worker.jobs (
    tenant_id TEXT NOT NULL,
    queue TEXT NOT NULL,
    id TEXT NOT NULL,
    sequence BIGINT GENERATED ALWAYS AS IDENTITY,
    payload JSONB NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts INTEGER NOT NULL CHECK (max_attempts > 0),
    backoff_ms BIGINT NOT NULL CHECK (backoff_ms >= 0),
    status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued','processing','completed','failed')),
    available_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    lease_token UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (tenant_id, queue, id),
    CHECK ((status = 'processing' AND lease_token IS NOT NULL AND lease_until IS NOT NULL) OR (status <> 'processing' AND lease_token IS NULL AND lease_until IS NULL)),
    CHECK (attempts <= max_attempts)
);
CREATE INDEX jobs_ready ON fl_worker.jobs (tenant_id, queue, available_at, sequence)
    WHERE status = 'queued';
CREATE INDEX jobs_leases ON fl_worker.jobs (tenant_id, queue, lease_until)
    WHERE status = 'processing';
ALTER TABLE fl_worker.jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE fl_worker.jobs FORCE ROW LEVEL SECURITY;
CREATE POLICY forklaunch_tenant_policy ON fl_worker.jobs
    USING (tenant_id = current_setting('forklaunch.tenant_id', true))
    WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));

CREATE INDEX jobs_terminal ON fl_worker.jobs (tenant_id, queue, status, updated_at, sequence)
    WHERE status IN ('completed', 'failed');

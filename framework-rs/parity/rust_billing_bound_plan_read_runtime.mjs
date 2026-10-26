// Bound Plan read: live HTTP, MCP and both SDKs (http_plan_read).
import {runBoundRuntime} from './fixtures/billing_bound_runtime.mjs';
runBoundRuntime('http_plan_read', 'Plan read');

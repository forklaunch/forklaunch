// Bound Subscription read: live HTTP, MCP and both SDKs (http_subscription_read).
import {runBoundRuntime} from './fixtures/billing_bound_runtime.mjs';
runBoundRuntime('http_subscription_read', 'Subscription read');

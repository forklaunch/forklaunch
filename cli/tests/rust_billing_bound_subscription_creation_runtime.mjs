// Bound Subscription creation: live HTTP, MCP and both SDKs (http_subscription_creation).
import {runBoundRuntime} from './fixtures/billing_bound_runtime.mjs';
runBoundRuntime('http_subscription_creation', 'Subscription creation');

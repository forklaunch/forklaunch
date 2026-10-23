// Bound Checkout: live HTTP, MCP and both SDKs (http_bound_checkout).
import {runBoundRuntime} from './fixtures/billing_bound_runtime.mjs';
runBoundRuntime('http_bound_checkout', 'Checkout');

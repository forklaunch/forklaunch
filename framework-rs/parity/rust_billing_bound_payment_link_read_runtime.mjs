// Bound PaymentLink read: live HTTP, MCP and both SDKs (http_payment_link_read).
import {runBoundRuntime} from './fixtures/billing_bound_runtime.mjs';
runBoundRuntime('http_payment_link_read', 'PaymentLink read');

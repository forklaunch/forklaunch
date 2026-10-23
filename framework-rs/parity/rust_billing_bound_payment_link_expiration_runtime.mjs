// Bound PaymentLink expiration: live HTTP, MCP and both SDKs (http_payment_link_expiration).
import {runBoundRuntime} from './fixtures/billing_bound_runtime.mjs';
runBoundRuntime('http_payment_link_expiration', 'PaymentLink expiration');

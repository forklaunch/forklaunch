// Bound PaymentLink expiration recovery: live HTTP, MCP and both SDKs (http_payment_link_expiration_recovery).
import {runBoundRuntime} from './fixtures/billing_bound_runtime.mjs';
runBoundRuntime('http_payment_link_expiration_recovery', 'PaymentLink expiration recovery');

// Bound PaymentLink creation: live HTTP, MCP and both SDKs (http_payment_link_creation).
import {runBoundRuntime} from './fixtures/billing_bound_runtime.mjs';
runBoundRuntime('http_payment_link_creation', 'PaymentLink creation');

#!/usr/bin/env node
/**
 * The ForkLaunch gateway mock, under its original name. Every feature's mock
 * (models, payments, email, sms, whatsapp, voice) now lives in
 * ./gateway-mock; see gateway-mock/server.mjs for settings and control
 * endpoints. Kept so existing docker-compose files and scripts keep working.
 */
import { startGatewayMock } from './gateway-mock/server.mjs';

const server = await startGatewayMock();
for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => server.close(() => process.exit(0)));
}

import { metricsDefinitions } from '@forklaunch/core/http';

export type Metrics = typeof metrics;

export const metrics = metricsDefinitions({
  // Request rate: total HTTP requests received.
  // Labels: service.name, application.id, api.name, http.request.method, http.route, http.response.status_code.
  http_requests_total: 'counter',

  // Latency: distribution of request durations in milliseconds.
  // Labels: service.name, application.id, api.name, http.request.method, http.route, http.response.status_code.
  http_request_duration_ms: 'histogram',

  // Error count: total HTTP requests that returned an error (4xx/5xx).
  // Labels: service.name, application.id, api.name, http.request.method, http.route, http.response.status_code.
  http_errors_total: 'counter',

  // Saturation: number of HTTP requests currently being processed.
  // Labels: service.name.
  http_requests_in_flight: 'upDownCounter'
});

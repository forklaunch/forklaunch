import {
  MetricsDefinition,
  metricsDefinitions,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { metrics } from '@forklaunch/blueprint-monitoring';

// The shared HTTP metrics plus MLSE's own safety and usage counters.
export const mlseMetrics = metricsDefinitions({
  ...metrics,
  // answers by kind (answer, boundary, emergency, ...) and query class
  mlse_answers_total: 'counter',
  // drafted sentences removed by citation or number verification
  mlse_sentences_removed_total: 'counter',
  // voice requests by outcome (answered, voice_disabled)
  mlse_voice_requests_total: 'counter'
});

export type MlseMetrics = typeof mlseMetrics;

// Adds to a counter when the collector defines it; test collectors created
// without metrics simply record nothing.
export function countMetric(
  collector: OpenTelemetryCollector<MetricsDefinition>,
  name: Exclude<keyof MlseMetrics, keyof typeof metrics>,
  value: number,
  attributes: Record<string, string>
): void {
  const metric = (collector.getMetric as (id: string) => unknown)(name) as
    | { add?: (value: number, attributes?: Record<string, string>) => void }
    | undefined;
  metric?.add?.(value, attributes);
}

import {
  MetricsDefinition,
  OpenTelemetryCollector
} from '@forklaunch/core/http';
import { ContentSourceProvider } from '@forklaunch/interfaces-mlse/interfaces';
import { SourceDescriptorDto } from '@forklaunch/interfaces-mlse/types';
import { PUBLIC_SOURCES } from '../domain/publicSources';

// The built-in provider every organization gets: free sources that permit
// commercial use. Licensed content a client adds is served by a separate
// provider, never mixed into this one.
export class PublicCorpusProvider implements ContentSourceProvider {
  protected openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>;

  constructor(
    openTelemetryCollector: OpenTelemetryCollector<MetricsDefinition>
  ) {
    this.openTelemetryCollector = openTelemetryCollector;
  }

  describe(): SourceDescriptorDto[] {
    this.openTelemetryCollector.debug('Describing public corpus sources');
    return PUBLIC_SOURCES.map((source) => ({ ...source }));
  }
}

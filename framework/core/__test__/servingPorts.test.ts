import { beforeEach, describe, expect, it } from 'vitest';
import {
  clearServingPorts,
  getServingPorts,
  registerServingPort
} from '../src/http/servingPorts';

beforeEach(() => clearServingPorts());

describe('serving port registry', () => {
  it('records what was bound, in port order', () => {
    registerServingPort({ port: 11000, protocol: 'ws', healthPath: '/health' });
    registerServingPort({
      port: 8000,
      protocol: 'http',
      healthPath: '/health'
    });

    expect(getServingPorts()).toEqual([
      { port: 8000, protocol: 'http', healthPath: '/health' },
      { port: 11000, protocol: 'ws', healthPath: '/health' }
    ]);
  });

  it('records the port actually bound, not a conventional one', () => {
    // Three separate code paths used to assume 11000 for websockets. The
    // whole point of registering is that a service which binds 12000 says 12000.
    registerServingPort({ port: 12000, protocol: 'ws', healthPath: '/health' });

    expect(getServingPorts()).toEqual([
      { port: 12000, protocol: 'ws', healthPath: '/health' }
    ]);
  });

  it('is idempotent per port', () => {
    // A double registration must not double-count: the export runs after the
    // synchronous body of server.ts, and a port may be registered on more than
    // one path.
    registerServingPort({
      port: 8000,
      protocol: 'http',
      healthPath: '/health'
    });
    registerServingPort({ port: 8000, protocol: 'http', healthPath: '/other' });

    expect(getServingPorts()).toHaveLength(1);
    expect(getServingPorts()[0].healthPath).toBe('/health');
  });

  it('ignores a port that is not a usable number', () => {
    // `Number(process.env.PORT)` is NaN when PORT is unset, and the export must
    // not carry a nonsense entry into the manifest.
    registerServingPort({ port: NaN, protocol: 'http', healthPath: '/health' });
    registerServingPort({ port: 0, protocol: 'http', healthPath: '/health' });
    registerServingPort({ port: -1, protocol: 'http', healthPath: '/health' });

    expect(getServingPorts()).toEqual([]);
  });

  it('hands back a copy, so a caller cannot mutate the registry', () => {
    registerServingPort({
      port: 8000,
      protocol: 'http',
      healthPath: '/health'
    });

    getServingPorts().push({
      port: 9999,
      protocol: 'http',
      healthPath: '/health'
    });

    expect(getServingPorts()).toHaveLength(1);
  });
});

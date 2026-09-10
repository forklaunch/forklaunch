/**
 * The ports this process serves traffic on.
 *
 * Deployment needs to know which ports to give a load balancer target group,
 * a listener rule and a container port mapping. Until now that was inferred
 * from environment variable NAMES — anything ending `_PORT`, minus a denylist
 * of dependency ports. That is a guess, and it was wrong in both directions:
 * a dependency's port got a container mapping it never needed, and a
 * scaffolded `WS_PORT=11000` that no code read provisioned a listener and a
 * target group for a port with no server behind it. Because the target group
 * was health-checked on a DIFFERENT port, it reported healthy while routing
 * into nothing.
 *
 * The process is the only thing that knows what it bound. Anything that binds
 * a port to serve traffic registers it here, and the value is whatever was
 * actually used — a websocket server moved to 12000 reports 12000, not the
 * 11000 that several code paths used to assume.
 *
 * Only ports that SERVE register. A database or cache the service connects to
 * never calls this, which is what retires the denylist rather than extending
 * it for every new dependency.
 */

export interface ServingPort {
  /** The port bound. */
  port: number;
  /** How it is spoken to. Drives the target group's protocol. */
  protocol: 'http' | 'ws';
  /**
   * A path on THIS port that answers 2xx to a plain HTTP GET.
   *
   * The load balancer health-checks the port it forwards to, so every serving
   * port needs one — including a websocket port, which must answer an ordinary
   * GET even though its real traffic is upgrades.
   */
  healthPath: string;
}

const registry: ServingPort[] = [];

/**
 * Record a port this process serves. Idempotent per port: a re-registration
 * for the same port is ignored rather than duplicated, so a restart or a
 * second call during export cannot double-count.
 */
export function registerServingPort(entry: ServingPort): void {
  if (!Number.isFinite(entry.port) || entry.port <= 0) return;
  if (registry.some((existing) => existing.port === entry.port)) return;
  registry.push(entry);
}

/** Every registered serving port, lowest first. */
export function getServingPorts(): ServingPort[] {
  return [...registry].sort((a, b) => a.port - b.port);
}

/** Test seam. Not for application code. */
export function clearServingPorts(): void {
  registry.length = 0;
}

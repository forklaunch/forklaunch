import { hashString } from '@forklaunch/common';
import { ClusterConfig } from '@forklaunch/core/http';
import cluster, { Worker } from 'node:cluster';
import fs from 'node:fs';
import http, { IncomingMessage, ServerResponse } from 'node:http';
import https from 'node:https';
import net from 'node:net';
import os from 'node:os';
import tls from 'node:tls';

export function startNodeCluster<
  ExpressApp extends (req: IncomingMessage, res: ServerResponse) => void
>(config: ClusterConfig<ExpressApp>) {
  const PORT = config.port;
  const HOST = config.host;
  const WORKERS = config.workerCount;
  const ROUTING_STRATEGY = config.routingStrategy || 'round-robin';
  const app = config.expressApp;
  const openTelemetryCollector = config.openTelemetryCollector;
  const SSL_CONFIG = config.ssl;

  if (WORKERS > os.cpus().length) {
    throw new Error('Worker count cannot be greater than the number of CPUs');
  }

  // Prepare SSL options if SSL config is provided
  let sslOptions: https.ServerOptions | undefined;
  if (SSL_CONFIG) {
    try {
      sslOptions = {
        key: fs.readFileSync(SSL_CONFIG.keyFile),
        cert: fs.readFileSync(SSL_CONFIG.certFile),
        ca: fs.readFileSync(SSL_CONFIG.caFile)
      };
      openTelemetryCollector.info(`SSL/TLS configuration loaded from files`);
    } catch (error) {
      openTelemetryCollector.error(
        'Failed to load SSL/TLS certificates:',
        error
      );
      // eslint-disable-next-line preserve-caught-error
      throw new Error('SSL/TLS configuration failed');
    }
  }

  if (cluster.isPrimary) {
    cluster.setupPrimary({ execArgv: process.execArgv });

    const protocol = SSL_CONFIG ? 'https' : 'http';
    openTelemetryCollector.info(
      `[primary ${process.pid}] starting ${WORKERS} workers with ${ROUTING_STRATEGY} routing on ${protocol}://${HOST}:${PORT}`
    );

    const workers = Array.from({ length: WORKERS }, (_, i) => {
      const worker = cluster.fork();

      worker.on(
        'message',
        (message: { type?: string; port?: number; index?: string }) => {
          if (message && message.type === 'worker-ready') {
            openTelemetryCollector.info(
              `Worker ${message.index || i} (PID: ${worker.process.pid}) ready`
            );
          }
        }
      );

      return worker;
    });
    let currentWorkerIndex = 0;

    function getWorker(remoteAddress?: string): Worker {
      switch (ROUTING_STRATEGY) {
        case 'sticky': {
          const ip = remoteAddress ?? '';
          const idx = hashString(ip) % workers.length;
          return workers[idx];
        }

        case 'random': {
          return workers[Math.floor(Math.random() * workers.length)];
        }

        case 'round-robin':
        default: {
          const worker = workers[currentWorkerIndex];
          currentWorkerIndex = (currentWorkerIndex + 1) % workers.length;
          return worker;
        }
      }
    }

    // Create TLS or TCP server based on SSL configuration
    const server = SSL_CONFIG
      ? tls.createServer(sslOptions!, (socket) => {
          const w = getWorker(socket.remoteAddress);
          w.send({ type: 'sticky-connection' }, socket);
        })
      : net.createServer({ pauseOnConnect: true }, (socket) => {
          const w = getWorker(socket.remoteAddress);
          w.send({ type: 'sticky-connection' }, socket);
        });

    server.on('error', (err) => {
      openTelemetryCollector.error('primary net server error:', err);
    });

    server.listen(PORT, HOST, () => {
      const protocol = SSL_CONFIG ? 'https' : 'http';
      openTelemetryCollector.info(
        `[primary ${process.pid}] listening on ${protocol}://${HOST}:${PORT}`
      );
    });

    cluster.on('exit', (worker) => {
      openTelemetryCollector.warn(
        `worker ${worker.process.pid} died; restarting`
      );
      const i = workers.findIndex((w) => w.id === worker.id);
      const replacement = cluster.fork();

      replacement.on(
        'message',
        (message: { type?: string; port?: number; index?: string }) => {
          if (message && message.type === 'worker-ready') {
            openTelemetryCollector.info(
              `Worker ${message.index || i} (PID: ${replacement.process.pid}) ready`
            );
          }
        }
      );

      if (i !== -1) workers[i] = replacement;
    });

    // Handle graceful shutdown
    let isShuttingDown = false;

    process.on('SIGINT', () => {
      if (isShuttingDown) return;
      isShuttingDown = true;

      openTelemetryCollector.info(
        'Received SIGINT, shutting down gracefully...'
      );

      workers.forEach((worker) => {
        worker.send({ type: 'shutdown' });
        worker.kill('SIGTERM');
      });

      setTimeout(() => {
        workers.forEach((worker) => worker.kill('SIGKILL'));
        process.exit(0);
      }, 5000);
    });
  } else {
    // Create HTTP or HTTPS server in worker based on SSL configuration
    const server = SSL_CONFIG
      ? https.createServer(sslOptions!, app)
      : http.createServer(app);

    const IDX = process.env.WORKER_INDEX ?? cluster.worker?.id ?? '0';

    process.on('message', (msg: { type: string }, socket: net.Socket) => {
      if (msg?.type === 'sticky-connection' && socket) {
        server.emit('connection', socket);
        socket.resume();
      } else if (msg?.type === 'shutdown') {
        openTelemetryCollector.info(
          `[worker ${process.pid}] received shutdown signal from primary`
        );
        server.close(() => {
          process.exit(0);
        });
      }
    });

    server.on('clientError', (err, socket) => {
      try {
        const response = SSL_CONFIG
          ? 'HTTP/1.1 400 Bad Request\r\n\r\n'
          : 'HTTP/1.1 400 Bad Request\r\n\r\n';
        socket.end(response);
      } catch {
        openTelemetryCollector.error('clientError', err);
      }
    });

    server.listen(0, () => {
      if (process.send) {
        process.send({ type: 'worker-ready', port: PORT, index: IDX });
      }
    });

    // Memory monitoring similar to Bun cluster
    setInterval(() => {
      const memUsage = process.memoryUsage();
      const heapUsedMB = Math.round(memUsage.heapUsed / 1024 / 1024);

      if (heapUsedMB > 100) {
        openTelemetryCollector.warn(
          `High memory usage on worker ${process.pid}: ${heapUsedMB} MB`
        );
      }
    }, 30000);

    // Error handling
    process.on('uncaughtException', (err) => {
      openTelemetryCollector.error(
        `Uncaught exception on worker ${process.pid}:`,
        err
      );
      process.exit(1);
    });

    process.on('unhandledRejection', (reason) => {
      openTelemetryCollector.error(
        `Unhandled rejection on worker ${process.pid}:`,
        reason
      );
      process.exit(1);
    });

    const protocol = SSL_CONFIG ? 'https' : 'http';
    openTelemetryCollector.info(
      `[worker ${process.pid}] ready (awaiting sockets from primary) - ${protocol}`
    );
  }
}

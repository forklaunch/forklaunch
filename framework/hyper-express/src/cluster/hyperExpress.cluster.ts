import { ClusterConfig } from '@forklaunch/core/http';
import { Server } from '@forklaunch/hyper-express-fork';
import cluster, { Worker } from 'node:cluster';
import * as os from 'node:os';
import * as uWS from 'uWebSockets.js';

export function startHyperExpressCluster(config: ClusterConfig<Server>) {
  const PORT = config.port;
  const HOST = config.host;
  const WORKERS = config.workerCount;
  const app = config.expressApp;
  const openTelemetryCollector = config.openTelemetryCollector;

  if (WORKERS > os.cpus().length) {
    throw new Error('Worker count cannot be greater than the number of CPUs');
  }

  if (cluster.isPrimary) {
    const workers = new Map<number, Worker>();
    let isShuttingDown = false;

    openTelemetryCollector.info(
      `[primary ${process.pid}] starting ${WORKERS} workers on ${HOST}:${PORT}. Using SO_REUSEPORT kernel level routing.`
    );

    function startWorker(i: number) {
      if (isShuttingDown) return;

      const worker = cluster.fork({
        WORKER_INDEX: i.toString(),
        PORT: PORT.toString(),
        HOST: HOST
      });

      workers.set(worker.id, worker);

      worker.on('exit', () => {
        if (isShuttingDown) {
          return;
        }
        workers.delete(worker.id);
        openTelemetryCollector.warn(
          `worker ${worker.process.pid} died; restarting in 2s`
        );
        setTimeout(() => {
          if (!isShuttingDown) {
            startWorker(i);
          }
        }, 2000);
      });

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
    }

    for (let i = 0; i < WORKERS; i++) {
      startWorker(i);
    }

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
    const WORKER_PORT = parseInt(process.env.PORT || '0');
    const IDX = process.env.WORKER_INDEX ?? cluster.worker?.id ?? '0';

    if (!WORKER_PORT) {
      openTelemetryCollector.error('Worker port not provided');
      process.exit(1);
    }

    let listenSocket: uWS.us_listen_socket | null = null;

    try {
      app.listen(WORKER_PORT, (socket: uWS.us_listen_socket) => {
        listenSocket = socket;
        if (process.send) {
          process.send({ type: 'worker-ready', port: WORKER_PORT, index: IDX });
        }
        openTelemetryCollector.info(
          `[worker ${process.pid}] listening on shared ${HOST}:${WORKER_PORT}`
        );
      });
    } catch (error: unknown) {
      openTelemetryCollector.error('Failed to start worker:', error);
      process.exit(1);
    }

    process.on('message', (msg: { type: string }) => {
      if (msg?.type === 'shutdown') {
        openTelemetryCollector.info(
          `[worker ${process.pid}] received shutdown signal from primary`
        );
        if (listenSocket) {
          uWS.us_listen_socket_close(listenSocket);
        }
        process.exit(0);
      }
    });

    process.on('SIGINT', () => {
      openTelemetryCollector.info(
        `[worker ${process.pid}] shutting down gracefully...`
      );
      if (listenSocket) {
        uWS.us_listen_socket_close(listenSocket);
      }
      process.exit(0);
    });

    setInterval(() => {
      const memUsage = process.memoryUsage();
      const heapUsedMB = Math.round(memUsage.heapUsed / 1024 / 1024);

      if (heapUsedMB > 100) {
        openTelemetryCollector.warn(
          `High memory usage on worker ${process.pid}: ${heapUsedMB} MB`
        );
      }
    }, 30000);

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
  }
}

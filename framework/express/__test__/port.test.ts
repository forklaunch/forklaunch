import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { SchemaValidator, string } from '@forklaunch/validator/typebox';
import express from 'express';
import { Server } from 'http';
import { afterAll, beforeAll, describe, expect, test } from 'vitest';
import { port } from '../index';

const typeboxSchemaValidator = SchemaValidator();
const openTelemetryCollector = new OpenTelemetryCollector('test');

describe('port function integration', () => {
  let server: Server;
  const app = express();
  app.use(express.json());
  const portNum = 6935; // Use a different port than other tests to avoid conflicts

  beforeAll(async () => {
    // defined handlers using port()
    const handlers = port(
      {
        method: 'get',
        path: '/port-test',
        basePath: '',
        schemaValidator: typeboxSchemaValidator,
        openTelemetryCollector: openTelemetryCollector
      },
      {
        name: 'TestPort',
        summary: 'Test Port Function',
        responses: {
          200: string
        }
      },
      async (_req, res) => {
        res.status(200).send('Hello Port');
      }
    );

    // Mount handlers to the app
    app.get('/port-test', ...handlers);

    server = await app.listen(portNum, () => {});
  });

  afterAll(async () => {
    server.close();
  });

  test('should successfully handle request on route registered via port()', async () => {
    const response = await fetch(`http://localhost:${portNum}/port-test`, {
      method: 'GET'
    });

    expect(response.status).toBe(200);
    expect(await response.text()).toBe('Hello Port');
  });
});

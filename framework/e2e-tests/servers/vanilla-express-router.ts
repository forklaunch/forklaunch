import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { forklaunchRouter, handlers } from '@forklaunch/express';
import {
  array,
  number,
  SchemaValidator,
  string,
  union
} from '@forklaunch/validator/zod';
import express from 'express';

const app = express();

app
  .get('/test', (req, res) => {
    res.send('Hello, world!');
  })
  .post('/test', (req, res) => {
    res.send('Hello, world!');
  });

const zodSchemaValidator = SchemaValidator();
const openTelemetryCollector = new OpenTelemetryCollector('test');

const flRouter = forklaunchRouter(
  '/test',
  zodSchemaValidator,
  openTelemetryCollector
);

const postHandler = handlers.post(
  zodSchemaValidator,
  '/sse',
  {
    name: 'Test',
    access: 'public',
    summary: 'Test Summary',
    body: {
      f: string,
      m: union([array(number), string])
    },
    responses: {
      200: {
        contentType: 'text/event-stream',
        event: {
          id: string,
          data: {
            message: string
          }
        }
      }
    }
  },
  (req, res) => {
    res.status(200).sseEmitter(async function* () {
      try {
        for (let i = 0; i < 100; i++) {
          yield {
            id: i.toString(),
            data: {
              message: `Hello World ${req.body.f}`
            }
          };
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
      } catch (e: unknown) {
        if (e instanceof Error) {
          res.status(500).send('There was an error');
        }
      }
      return;
    });
  }
);

flRouter.post('/sse', postHandler);

app.use(flRouter.basePath, flRouter.internal);

app.listen(3000, () => {
  console.log('Server is running on port 3000');
});

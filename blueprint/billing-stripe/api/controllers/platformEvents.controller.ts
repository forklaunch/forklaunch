import { handlers, schemaValidator, string } from '@forklaunch/blueprint-core';
import {
  PlatformEventVerificationError,
  verifyPlatformEvent
} from '@forklaunch/core/http';
import { platformEventHandlers } from '../platformEvents';

/**
 * Receives an event the ForkLaunch platform delivered, verifies it was signed
 * with this instance's key, and hands it to the feature's handler
 * (api/platformEvents/<feature>.ts). Deliveries can repeat: handlers dedupe
 * on event.id. A non-2xx answer makes the platform retry.
 */
export const receivePlatformEvent = handlers.post(
  schemaValidator,
  '/:feature',
  {
    name: 'ReceivePlatformEvent',
    access: 'public',
    summary: 'Receives a signed event from the ForkLaunch platform',
    params: { feature: string },
    body: schemaValidator.unknown,
    responses: { 200: string, 401: string, 404: string }
  },
  async (req, res) => {
    const handler = platformEventHandlers[req.params.feature];
    if (!handler) {
      res.status(404).send(`No handler for ${req.params.feature} events`);
      return;
    }
    let event;
    try {
      event = verifyPlatformEvent({
        method: 'POST',
        path: `/platform-events/${req.params.feature}`,
        headers: req.headers as Record<string, string | string[] | undefined>,
        body: (req as { _rawBody?: Buffer })._rawBody ?? req.body
      });
    } catch (error) {
      if (error instanceof PlatformEventVerificationError) {
        res.status(401).send(error.message);
        return;
      }
      throw error;
    }
    await handler(event);
    res.status(200).send('ok');
  }
);

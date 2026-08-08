import { forklaunchRouter, schemaValidator } from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  createPayment,
  getPayment
} from '../controllers/payment.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const paymentRouter = forklaunchRouter(
  '/payment',
  schemaValidator,
  openTelemetryCollector
);

export const createPaymentRoute = paymentRouter.post('/', createPayment);
export const getPaymentRoute = paymentRouter.get('/:id', getPayment);

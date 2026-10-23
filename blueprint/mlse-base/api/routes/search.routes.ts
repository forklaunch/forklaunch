import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { images, search, spelling, suggestions } from '../controllers/search.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const searchRouter = forklaunchRouter(
  '/search',
  schemaValidator,
  openTelemetryCollector
);

export const searchRoute = searchRouter.get('/', search);
export const searchSuggestionsRoute = searchRouter.get('/suggestions', suggestions);
export const searchSpellingRoute = searchRouter.get('/spelling', spelling);
export const searchImagesRoute = searchRouter.get('/images', images);

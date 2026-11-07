import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  deleteSavedSearch,
  listSavedSearches,
  saveSearch,
  searchHistory
} from '../controllers/savedSearch.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const savedSearchRouter = forklaunchRouter(
  '/saved-search',
  schemaValidator,
  openTelemetryCollector
);

// /history is registered before /:id-style routes so it is not read as an id
export const searchHistoryRoute = savedSearchRouter.get('/history', searchHistory);
export const listSavedSearchesRoute = savedSearchRouter.get('/', listSavedSearches);
export const saveSearchRoute = savedSearchRouter.post('/', saveSearch);
export const deleteSavedSearchRoute = savedSearchRouter.delete('/:id', deleteSavedSearch);

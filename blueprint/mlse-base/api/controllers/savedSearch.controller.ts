import {
  array,
  handlers,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { SavedSearchNotFoundError } from '../../domain/services/savedSearch.service';

const savedSearchServiceFactory = ci.scopedResolver(tokens.SavedSearchService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

const hmacAuth = { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } };

const SavedSchema = {
  id: string,
  name: string,
  query: string,
  topicSlug: optional(string),
  createdAt: string
};

export const listSavedSearches = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Saved Searches',
    access: 'internal',
    summary: 'The searches a user has saved',
    auth: hmacAuth,
    query: { organizationId: string, userId: string },
    responses: { 200: array(SavedSchema) }
  },
  async (req, res) => {
    res.status(200).json(await savedSearchServiceFactory().list(req.query.organizationId, req.query.userId));
  }
);

export const saveSearch = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Save Search',
    access: 'internal',
    summary: 'Saves a search to run again',
    auth: hmacAuth,
    body: {
      organizationId: string,
      userId: string,
      name: string,
      query: string,
      topicSlug: optional(string)
    },
    responses: { 200: SavedSchema, 400: string }
  },
  async (req, res) => {
    const { organizationId, userId, name, query, topicSlug } = req.body;
    if (!name.trim() || name.length > 200 || !query.trim() || query.length > 2000) {
      res.status(400).send('name must be 1 to 200 characters and query 1 to 2000');
      return;
    }
    res.status(200).json(await savedSearchServiceFactory().save(organizationId, userId, name, query, topicSlug));
  }
);

export const deleteSavedSearch = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Delete Saved Search',
    access: 'internal',
    summary: 'Deletes a saved search',
    auth: hmacAuth,
    params: { id: string },
    query: { organizationId: string, userId: string },
    responses: { 200: { id: string }, 404: string }
  },
  async (req, res) => {
    try {
      await savedSearchServiceFactory().remove(req.query.organizationId, req.query.userId, req.params.id);
      res.status(200).json({ id: req.params.id });
    } catch (error) {
      if (error instanceof SavedSearchNotFoundError) {
        res.status(404).send(error.message);
        return;
      }
      throw error;
    }
  }
);

export const searchHistory = handlers.get(
  schemaValidator,
  '/history',
  {
    name: 'Search History',
    access: 'internal',
    summary:
      'A user’s 50 most recent searches; query text is absent for patient, prescription and emergency queries and after 90 days',
    auth: hmacAuth,
    query: { organizationId: string, userId: string },
    responses: {
      200: array({
        id: string,
        query: optional(string),
        queryClass: string,
        channel: string,
        answerId: optional(string),
        createdAt: string
      })
    }
  },
  async (req, res) => {
    res.status(200).json(await savedSearchServiceFactory().history(req.query.organizationId, req.query.userId));
  }
);

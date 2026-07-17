import {
  array,
  handlers,
  IdSchema,
  IdsSchema,
  schemaValidator,
  string
} from '../../schema';
import { ci, tokens } from '../../bootstrapper';
import {
  CreateVariantMapper,
  UpdateVariantMapper,
  VariantMapper
} from '../../domain/mappers/variant.mappers';

const serviceFactory = ci.scopedResolver(tokens.VariantService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

export const createVariant = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Variant',
    access: 'internal',
    summary: 'Create a variant',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: CreateVariantMapper.schema,
    responses: { 200: VariantMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().createVariant(req.body));
  }
);

export const getVariant = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Variant',
    access: 'internal',
    summary: 'Get a variant',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: IdSchema,
    responses: { 200: VariantMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().getVariant(req.params));
  }
);

export const listVariantsByProduct = handlers.get(
  schemaValidator,
  '/product/:productId',
  {
    name: 'List Variants By Product',
    access: 'internal',
    summary: 'List variants for a product',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: { productId: string },
    responses: { 200: array(VariantMapper.schema) }
  },
  async (req, res) => {
    res
      .status(200)
      .json(await serviceFactory().listVariantsByProduct(req.params));
  }
);

export const updateVariant = handlers.put(
  schemaValidator,
  '/',
  {
    name: 'Update Variant',
    access: 'internal',
    summary: 'Update a variant',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    body: UpdateVariantMapper.schema,
    responses: { 200: VariantMapper.schema }
  },
  async (req, res) => {
    res.status(200).json(await serviceFactory().updateVariant(req.body));
  }
);

export const deleteVariant = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Delete Variant',
    access: 'internal',
    summary: 'Delete a variant',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    params: IdSchema,
    responses: { 200: string }
  },
  async (req, res) => {
    await serviceFactory().deleteVariant(req.params);
    res.status(200).send('Deleted variant');
  }
);

export const listVariants = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Variants',
    access: 'internal',
    summary: 'List variants',
    auth: { hmac: { secretKeys: { default: HMAC_SECRET_KEY } } },
    query: IdsSchema,
    responses: { 200: array(VariantMapper.schema) }
  },
  async (req, res) => {
    res
      .status(200)
      .json(
        await serviceFactory().listVariants(
          req.query.ids ? (req.query as { ids: string[] }) : undefined
        )
      );
  }
);

import { SchemaValidator, schemaValidator } from '../../schema';
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import {
  BaseCartServiceSchemas,
  BaseInventoryServiceSchemas,
  BaseOrderServiceSchemas,
  BasePaymentServiceSchemas,
  BaseProductServiceSchemas,
  BaseVariantServiceSchemas
} from '@forklaunch/implementation-ecommerce-base/schemas';

// Entity ids are fp.string() (uuid-shaped strings assigned via uuid()), not
// mikro-orm's uuid column type — uuidId: false matches sqlBaseProperties.
const schemas = mapServiceSchemas(
  {
    CartSchemas: BaseCartServiceSchemas<SchemaValidator>,
    InventorySchemas: BaseInventoryServiceSchemas<SchemaValidator>,
    OrderSchemas: BaseOrderServiceSchemas<SchemaValidator>,
    PaymentSchemas: BasePaymentServiceSchemas<SchemaValidator>,
    ProductSchemas: BaseProductServiceSchemas<SchemaValidator>,
    VariantSchemas: BaseVariantServiceSchemas<SchemaValidator>
  },
  {
    validator: schemaValidator,
    uuidId: false
  }
);

export const {
  CartSchemas,
  InventorySchemas,
  OrderSchemas,
  PaymentSchemas,
  ProductSchemas,
  VariantSchemas
} = schemas;

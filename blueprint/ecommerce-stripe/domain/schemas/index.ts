import { SchemaValidator, schemaValidator } from '../../schema';
import { mapServiceSchemas } from '@forklaunch/core/mappers';
import { BaseVariantServiceSchemas } from '@forklaunch/implementation-ecommerce-base/schemas';

// Entity ids are fp.string() (uuid-shaped strings assigned via uuid()), not
// mikro-orm's uuid column type — uuidId: false matches sqlBaseProperties.
const schemas = mapServiceSchemas(
  {
    VariantSchemas: BaseVariantServiceSchemas<SchemaValidator>
    // Remaining entities' schemas are added incrementally as each PR lands.
  },
  {
    validator: schemaValidator,
    uuidId: false
  }
);

export const { VariantSchemas } = schemas;

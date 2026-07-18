import { isTrue } from '@forklaunch/common';
import { describe, expect, it } from 'vitest';
import {
  CreateVariantDto,
  UpdateVariantDto,
  VariantDto
} from '@forklaunch/interfaces-ecommerce/types';
import { testSchemaEquality } from '@forklaunch/internal';
import {
  VariantSchema as TypeboxVariantSchema,
  UpdateVariantSchema as TypeboxUpdateVariantSchema
} from '../domain/schemas/typebox/variant.schema';
import { CreateVariantSchema as TypeboxCreateVariantSchema } from '../domain/schemas/typebox/variant.schema';
import {
  VariantSchema as ZodVariantSchema,
  UpdateVariantSchema as ZodUpdateVariantSchema
} from '../domain/schemas/zod/variant.schema';
import { CreateVariantSchema as ZodCreateVariantSchema } from '../domain/schemas/zod/variant.schema';

const zodUpdateVariantSchema = ZodUpdateVariantSchema({ uuidId: false });
const typeboxUpdateVariantSchema = TypeboxUpdateVariantSchema({ uuidId: false });
const zodVariantSchema = ZodVariantSchema({ uuidId: false });
const typeboxVariantSchema = TypeboxVariantSchema({ uuidId: false });

// Remaining entities' sample data + schema instantiations are added
// incrementally as each entity's PR lands.

describe('schema equality', () => {
  it('should be equal for variant', () => {
    expect(
      isTrue(
        testSchemaEquality<CreateVariantDto>()(
          ZodCreateVariantSchema,
          TypeboxCreateVariantSchema,
          {
            productId: 'prod-1',
            externalId: 'var-1',
            sku: '12345',
            title: 'Black',
            optionValues: { Color: 'Black' },
            priceCents: 1999,
            compareAtPriceCents: 2499,
            requiresShipping: true
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<UpdateVariantDto>()(
          zodUpdateVariantSchema,
          typeboxUpdateVariantSchema,
          {
            id: 'test',
            priceCents: 999
          }
        )
      )
    ).toBeTruthy();

    expect(
      isTrue(
        testSchemaEquality<VariantDto>()(zodVariantSchema, typeboxVariantSchema, {
          id: 'test',
          productId: 'prod-1',
          externalId: 'var-1',
          title: 'Black',
          priceCents: 1999
        })
      )
    ).toBeTruthy();
  });

  // Remaining entities' equality tests are added incrementally as each PR lands.
});

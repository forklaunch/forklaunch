import { Variant } from '../../persistence/entities';

// variant entity types
export type BaseVariantEntities = {
  VariantMapper: { '~entity': (typeof Variant)['~entity'] };
  CreateVariantMapper: { '~entity': (typeof Variant)['~entity'] };
  UpdateVariantMapper: { '~entity': (typeof Variant)['~entity'] };
};

// Remaining entities' Entity aggregates are added incrementally as each PR lands.

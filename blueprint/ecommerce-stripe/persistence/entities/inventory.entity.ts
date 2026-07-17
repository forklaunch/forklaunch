import { sqlBaseProperties } from '../../../core/persistence/sql.base.properties';
import { defineComplianceEntity, fp } from '@forklaunch/core/persistence';

export const Inventory = defineComplianceEntity({
  name: 'Inventory',
  properties: {
    ...sqlBaseProperties,
    variantId: fp.string().unique().compliance('none'),
    stock: fp.integer().compliance('none')
  }
});

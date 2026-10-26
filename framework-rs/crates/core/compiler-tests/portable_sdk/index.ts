import type { ItemsSdkClient } from './sdk';
import { universalSdk } from '@forklaunch/universal-sdk';
export type * from './sdk';
export const clientItemsSdkClient = universalSdk<ItemsSdkClient>;

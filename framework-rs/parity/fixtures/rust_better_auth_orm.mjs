// Load actual entity definitions, compliance types and MikroORM. No adapter or
// field serialization behavior is replaced by the fixture.
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import crypto from 'node:crypto';
import { actualValidator } from '../rust_actual_validator_fixture.mjs';
export async function betterAuthOrm(root, dependencies, resolveAdditional, sameRealm = false, entityIndex = 'blueprint/iam-better-auth/persistence/entities/index.ts') {
  const require = createRequire(path.join(dependencies, 'package.json'));
  const core = await import(pathToFileURL(require.resolve('@mikro-orm/core')));
  const { MikroORM } = await import(
    pathToFileURL(require.resolve('@mikro-orm/postgresql'))
  );
  let persistence;
  const actual = actualValidator(root, (specifier, _file, load) => {
    const additional = resolveAdditional?.(specifier, _file, load);
    if (additional !== undefined) return additional;
    if (specifier === '@mikro-orm/core') return core;
    if (specifier === 'crypto') return { default: crypto, ...crypto };
    if (specifier === 'uuid') return { v4: crypto.randomUUID };
    if (specifier === '@forklaunch/common')
      return {
        ...load(path.join(root, 'framework/common/src/safeStringify.ts')),
        ...load(path.join(root, 'framework/common/src/safeParse.ts'))
      };
    if (specifier === '@forklaunch/core/persistence') return persistence;
    if (specifier === '@forklaunch/blueprint-core')
      return load(
        path.join(root, 'blueprint/core/persistence/sql.base.properties.ts')
      );
  }, { sameRealm });
  const load = (file) => actual.load(path.join(root, file));
  persistence = {
    ...load('framework/core/src/persistence/compliancePropertyBuilder.ts'),
    ...load('framework/core/src/persistence/defineComplianceEntity.ts'),
    ...load('framework/core/src/persistence/complianceTypes.ts'),
    ...load('framework/core/src/persistence/encryptedType.ts'),
    ...load('framework/core/src/persistence/fieldEncryptor.ts'),
    ...load('framework/core/src/persistence/compliantField.ts')
  };
  return {
    MikroORM,
    core,
    persistence,
    entities: load(entityIndex),
    load
  };
}

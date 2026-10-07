export default {
  entry: { cli: 'src/index.ts' },
  format: ['esm'],
  banner: { js: '#!/usr/bin/env bun' },
  splitting: false,
  clean: true,
  dts: false
};

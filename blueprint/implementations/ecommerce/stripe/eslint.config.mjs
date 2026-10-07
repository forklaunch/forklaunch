import pluginJs from '@eslint/js';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default [
  { files: ['**/*.{ts,tsx}'] },
  {
    ignores: [
      '**/*tests/**/*',
      '**/*dist/**/*',
      '**/*lib/**/*',
      '**/*node_modules/**/*'
    ]
  },
  { languageOptions: { globals: globals.browser } },
  pluginJs.configs.recommended,
  ...tseslint.configs.recommended
];

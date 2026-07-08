import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import prettier from 'eslint-config-prettier';
import globals from 'globals';

/** @type {import('eslint').Linter.Config[]} */
export default [
  js.configs.recommended,
  ...svelte.configs['flat/recommended'],
  prettier,
  ...svelte.configs['flat/prettier'],
  {
    languageOptions: {
      globals: { ...globals.browser }
    },
    rules: {
      'no-unused-vars': ['error', { varsIgnorePattern: '^(_|unused)' }]
    }
  },
  {
    files: ['*.config.js', 'svelte.config.js'],
    languageOptions: {
      globals: { ...globals.node }
    }
  },
  {
    // Svelte 5 runes are compiler globals in .svelte.js modules
    files: ['**/*.svelte.js'],
    languageOptions: {
      globals: {
        $state: 'readonly',
        $derived: 'readonly',
        $effect: 'readonly',
        $props: 'readonly',
        $bindable: 'readonly',
        $inspect: 'readonly'
      }
    }
  },
  {
    ignores: [
      'node_modules/',
      'build/',
      '.svelte-kit/',
      'src-tauri/',
      'thumbnailer-design/',
      'dist/',
      '_bmad/',
      '_bmad-output/'
    ]
  }
];

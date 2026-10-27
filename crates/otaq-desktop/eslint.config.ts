// eslint-disable-next-line @typescript-eslint/triple-slash-reference -- ambient types for two untyped plugins; the linter's default project for config files doesn't load "include"
/// <reference path="./types/untyped-plugins.d.ts" />
import comments from "@eslint-community/eslint-plugin-eslint-comments";
import js from "@eslint/js";
import importPlugin from "eslint-plugin-import";
import jsxA11y from "eslint-plugin-jsx-a11y";
import reactHooks from "eslint-plugin-react-hooks";
import { defineConfig } from "eslint/config";
import globals from "globals";
import tseslint from "typescript-eslint";

export default defineConfig(
  {
    ignores: [
      "ui/dist/**",
      "ui/src/generated/**",
      "ui/src/theme/generated/**",
      "gen/**",
      "node_modules/**",
      "types/**",
    ],
  },
  js.configs.recommended,
  tseslint.configs.strictTypeChecked,
  reactHooks.configs.flat.recommended,
  {
    languageOptions: {
      globals: globals.browser,
      parserOptions: {
        projectService: {
          allowDefaultProject: ["*.config.ts", "scripts/*.ts"],
          defaultProject: "tsconfig.node.json",
        },
        tsconfigRootDir: import.meta.dirname,
      },
    },
    linterOptions: { reportUnusedDisableDirectives: "error" },
    plugins: { import: importPlugin, "eslint-comments": comments },
    settings: { "import/resolver": { typescript: true, node: true } },
    rules: {
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
      "@typescript-eslint/switch-exhaustiveness-check": [
        "error",
        { considerDefaultExhaustiveForUnions: false, requireDefaultForNonUnion: true },
      ],
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
      "import/no-extraneous-dependencies": [
        "error",
        { devDependencies: ["vite.config.ts", "eslint.config.ts", "**/*.test.ts"], includeTypes: true },
      ],
      // A disabled rule needs a written reason (d0075 point 5): `-- reason` after the directive.
      "eslint-comments/require-description": ["error", { ignore: [] }],
      "eslint-comments/no-unlimited-disable": "error",
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["@tauri-apps/*"],
              message: "Only ui/src/tauri.ts names Tauri (UIT-02); import from it instead.",
            },
          ],
        },
      ],
    },
  },
  {
    files: ["ui/src/**/*.tsx"],
    ...jsxA11y.flatConfigs.strict,
  },
  {
    files: ["ui/src/tauri.ts"],
    rules: { "no-restricted-imports": "off" },
  },
  {
    files: ["*.config.ts"],
    languageOptions: { globals: globals.node },
  },
);

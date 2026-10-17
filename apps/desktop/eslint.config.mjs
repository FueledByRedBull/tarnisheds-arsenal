import hooks from "eslint-plugin-react-hooks";
import { defineConfig } from "eslint/config";
import tseslint from "typescript-eslint";

export default defineConfig(
  { ignores: ["dist/", "src-tauri/", "test-results/"] },
  {
    files: ["**/*.{ts,tsx}"],
    extends: [tseslint.configs.recommendedTypeChecked],
    languageOptions: {
      parserOptions: { project: ["./tsconfig.json", "./tsconfig.tests.json"], tsconfigRootDir: import.meta.dirname },
    },
    rules: {
      // Mock backends and test doubles stand in for async APIs; `async` keeps their contract.
      "@typescript-eslint/require-await": "off",
      // Forwarding a caught rejection keeps its original reason.
      "@typescript-eslint/prefer-promise-reject-errors": ["error", { allowThrowingUnknown: true }],
    },
  },
  {
    files: ["**/*.mjs"],
    extends: [tseslint.configs.recommended],
  },
  {
    rules: {
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_", ignoreRestSiblings: true }],
    },
  },
  {
    // Tests read JSON fixtures and page probes injected through `window`, which are untyped by design.
    files: ["src/**/*.test.ts", "tests/**/*.ts"],
    rules: {
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-unsafe-argument": "off",
      "@typescript-eslint/no-unsafe-assignment": "off",
      "@typescript-eslint/no-unsafe-call": "off",
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-return": "off",
    },
  },
  {
    files: ["src/**/*.{ts,tsx}", "tests/**/*.ts"],
    // The React Compiler rules (refs, purity, set-state-in-effect, ...) plus the classic two.
    extends: [hooks.configs.flat["recommended-latest"]],
    rules: { "react-hooks/exhaustive-deps": "error" },
  },
);

import tsParser from "@typescript-eslint/parser";
import hooks from "eslint-plugin-react-hooks";

export default [{
  files: ["src/**/*.{ts,tsx}", "tests/**/*.ts"],
  languageOptions: { parser: tsParser, parserOptions: { ecmaFeatures: { jsx: true } } },
  plugins: { "react-hooks": hooks },
  rules: {
    // The React Compiler rules (refs, purity, set-state-in-effect, ...) plus the classic two.
    ...hooks.configs["recommended-latest"].rules,
    "react-hooks/exhaustive-deps": "error",
  },
}];

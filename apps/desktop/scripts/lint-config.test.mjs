import { test } from "node:test";
import assert from "node:assert/strict";
import { ESLint } from "eslint";

test("the frontend lint gate rejects conditional Hooks, stale effect dependencies and floating promises", async () => {
  // Under CI=true typescript-eslint lints the on-disk file instead of this text, so keep it out of single-run mode.
  const eslint = new ESLint({ overrideConfig: { languageOptions: { parserOptions: { disallowAutomaticSingleRunInference: true } } } });
  // Type-aware rules only lint files in a tsconfig project, so the text stands in for a real source file.
  const [result] = await eslint.lintText(`
    import { useEffect, useState } from "react";
    export function Example({ ready, value }: { ready: boolean; value: string }) {
      if (ready) useState(0);
      useEffect(() => { document.title = value; }, []);
      Promise.resolve(value);
      return null;
    }
  `, { filePath: "src/main.tsx" });
  assert.deepEqual(new Set(result.messages.map(message => message.ruleId)),
    new Set(["react-hooks/rules-of-hooks", "react-hooks/exhaustive-deps", "@typescript-eslint/no-floating-promises"]));
  assert.equal(result.errorCount, 3);
});

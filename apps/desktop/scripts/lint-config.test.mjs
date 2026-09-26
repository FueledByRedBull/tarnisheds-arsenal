import { test } from "node:test";
import assert from "node:assert/strict";
import { ESLint } from "eslint";

test("the frontend lint gate rejects conditional Hooks and stale effect dependencies", async () => {
  const eslint = new ESLint();
  const [result] = await eslint.lintText(`
    import { useEffect, useState } from "react";
    export function Example({ ready, value }: { ready: boolean; value: string }) {
      if (ready) useState(0);
      useEffect(() => { document.title = value; }, []);
      return null;
    }
  `, { filePath: "src/example.tsx" });
  assert.deepEqual(new Set(result.messages.map(message => message.ruleId)),
    new Set(["react-hooks/rules-of-hooks", "react-hooks/exhaustive-deps"]));
  assert.equal(result.errorCount, 2);
});

import AxeBuilder from "@axe-core/playwright";
import { test, type Page } from "@playwright/test";

// WCAG 2.2 A/AA rules over a surface (the whole page, or the `include` selector).
export async function violations(page: Page, include?: string) {
  const builder = new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]);
  const { violations, incomplete } = await (include ? builder.include(include) : builder).analyze();
  // Rules axe could not decide (contrast over gradients, overlays) need a human look.
  for (const result of incomplete) {
    test.info().annotations.push({ type: "axe-incomplete", description: `${result.id}: ${result.nodes.map((node) => node.target.join(" ")).join(", ")}` });
  }
  return violations.map((violation) => ({
    id: violation.id,
    impact: violation.impact,
    targets: violation.nodes.map((node) => node.target.join(" ")),
  }));
}

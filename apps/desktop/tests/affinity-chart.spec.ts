import { expect, test } from "@playwright/test";

test("affinity chart uses one padded domain for flat-series axes and grid", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();

  await page.evaluate(async () => {
    const { api } = await import("/src/lib/api.ts");
    api.startAffinityWatch = async () => ({ jobId: "browser-flat-affinity" });
    api.affinityWatchStatus = async (jobId) => ({
      progress: null,
      finished: {
        jobId,
        cancelled: false,
        error: null,
        payload: {
          lines: [{
            affinity: "Flat",
            points: [
              { level: 10, metric: 500, solved: null },
              { level: 11, metric: 500, solved: null },
            ],
            startMetric: 500,
            endMetric: 500,
            finalBuild: null,
          }],
          breakpoints: [],
        },
      },
    });
  });

  await page.getByRole("navigation").getByRole("button", { name: "Affinity Watch", exact: true }).click();
  await page.getByRole("button", { name: "Watch affinities", exact: true }).click();
  await expect(page.locator(".analysis-progress")).toHaveAttribute("data-analysis-status", "completed");
  await expect(page.locator(".affinity-y-axis span")).toHaveText(["510.0", "500.0", "490.0"]);
  await expect(page.locator(".affinity-grid-line")).toHaveCount(3);
  expect(await page.locator(".affinity-grid-line").evaluateAll((lines) => lines.map((line) => Number(line.getAttribute("y1"))))).toEqual([14, 110, 206]);
  expect(await page.locator(".affinity-series-point").evaluateAll((points) => points.map((point) => Number(point.getAttribute("cy"))))).toEqual([110, 110]);
  const axisDeltas = await page.locator(".affinity-plot").evaluate((plot) => {
    const centers = (elements: Element[]) => elements.map((element) => {
      const box = element.getBoundingClientRect();
      return (box.top + box.bottom) / 2;
    });
    const labels = centers(Array.from(plot.querySelectorAll(".affinity-y-axis span")));
    const grid = centers(Array.from(plot.querySelectorAll(".affinity-grid-line")));
    return labels.map((center, index) => Math.abs(center - grid[index]));
  });
  expect(Math.max(...axisDeltas)).toBeLessThan(1);
});

import { expect, test } from "@playwright/test";

for (const outcome of ["cancelled", "failed"] as const) {
  test(`keeps the saved comparison when its upgrade series is ${outcome}`, async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows", { exact: true })).toBeVisible();
    await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Save new", exact: true }).click();
    await page.reload();
    await page.getByText("Snapshot loaded", { exact: true }).waitFor();
    await page.getByRole("button", { name: "Load", exact: true }).click();
    const savedTarget = await page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const { api } = await import("/src/lib/api.ts");
      const target = useDesktopStore.getState().compareTarget!;
      const probe = { calls: 0, fail: () => {} };
      api.buildUpgradeSeries = (_base, _row, _maxUpgrade, signal) => new Promise((_resolve, reject) => {
        probe.calls += 1;
        probe.fail = () => reject(new Error("saved comparison upgrade failed"));
        signal.addEventListener("abort", () => reject(new Error("cancelled")), { once: true });
      });
      Object.assign(window, { savedSeriesProbe: probe });
      return { weaponName: target.weaponName, affinity: target.affinity };
    });
    await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
    await page.waitForFunction(() => (window as any).savedSeriesProbe.calls > 0);
    if (outcome === "failed") {
      await page.evaluate(() => (window as any).savedSeriesProbe.fail());
      await expect(page.locator('.error-strip[role="alert"]')).toContainText("saved comparison upgrade failed");
    }
    await page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true }).click();
    await expect(page.locator(".path-lane").nth(1)).toContainText(`${savedTarget.weaponName} / ${savedTarget.affinity}`);
    expect(await page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const target = useDesktopStore.getState().compareTarget;
      return target && { weaponName: target.weaponName, affinity: target.affinity };
    })).toEqual(savedTarget);
  });
}

for (const withPins of [false, true]) {
  test(`restores the saved active comparison through Compare and Paths ${withPins ? "without replacing pins" : "without pins"}`, async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows", { exact: true })).toBeVisible();
    await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    await page.getByRole("navigation").getByRole("button", { name: "Rankings", exact: true }).click();
    const savedTarget = await page.evaluate(async withPins => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const state = useDesktopStore.getState();
      const target = state.rows[2];
      useDesktopStore.setState({ compareTarget: target, compareBench: withPins ? [state.rows[1]] : [] });
      return { weaponName: target.weaponName, affinity: target.affinity };
    }, withPins);
    await page.getByRole("button", { name: "Save new", exact: true }).click();
    await page.reload();
    await page.getByText("Snapshot loaded", { exact: true }).waitFor();
    await page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      useDesktopStore.getState().patchCompareControls({ weaponName: "Claymore", matchSelectedAow: false });
    });
    await page.getByRole("button", { name: "Load", exact: true }).click();
    await page.locator(".result-row-full").click();
    await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    await expect.poll(() => page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      const target = useDesktopStore.getState().compareTarget;
      return target && { weaponName: target.weaponName, affinity: target.affinity };
    })).toEqual(savedTarget);
    await expect(page.getByRole("combobox", { name: "Compare Weapon", exact: true })).not.toHaveValue("Claymore");
    const pins = await page.evaluate(async () => {
      const { useDesktopStore } = await import("/src/lib/state.ts");
      return useDesktopStore.getState().compareBench;
    });
    expect(pins).toHaveLength(withPins ? 1 : 0);
    await page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true }).click();
    await expect(page.locator(".path-lane").nth(1)).toContainText(`${savedTarget.weaponName} / ${savedTarget.affinity}`);
    await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    await page.getByRole("navigation").getByRole("button", { name: "Paths", exact: true }).click();
    await expect(page.locator(".path-lane").nth(1)).toContainText(`${savedTarget.weaponName} / ${savedTarget.affinity}`);
    await page.getByRole("navigation").getByRole("button", { name: "Compare", exact: true }).click();
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    await page.getByRole("checkbox", { name: "Smithing", exact: true }).uncheck();
    await page.getByRole("checkbox", { name: "Somber", exact: true }).uncheck();
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    await page.getByRole("checkbox", { name: "Smithing", exact: true }).check();
    await page.getByRole("checkbox", { name: "Somber", exact: true }).check();
    await expect(page.getByText("Comparison current", { exact: true })).toBeVisible();
    await expect(page.getByRole("group", { name: "Saved target", exact: true })).toHaveCount(0);
  });
}

test("rejects malformed comparison pins without importing them or disabling search", async ({ page }) => {
  const crashes: string[] = [];
  page.on("pageerror", error => crashes.push(error.message));
  await page.goto("/");
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  const malformed = await page.evaluate(() => {
    const key = Object.keys(localStorage).find(key => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
    const preset = JSON.parse(localStorage.getItem(key)!);
    preset.compareBench = [null];
    return JSON.stringify(preset);
  });
  const before = await page.evaluate(() => JSON.stringify(localStorage));
  await page.locator(".saved-build-import summary").click();
  await page.getByRole("textbox", { name: "Import JSON or Share Text", exact: true }).fill(malformed);
  await expect(page.getByRole("button", { name: "Import", exact: true })).toBeDisabled();
  await expect(page.getByText(/compareBench\[0\] must be a build/)).toBeVisible();
  expect(await page.evaluate(() => JSON.stringify(localStorage))).toBe(before);
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  expect(crashes).toEqual([]);
});

for (const stale of [false, true]) {
  test(`loads ${stale ? "stale inputs" : "a saved result"} when optional comparison storage fails`, async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
    await page.getByRole("button", { name: "Save new", exact: true }).click();
    const stored = await page.evaluate((stale) => {
      const key = Object.keys(localStorage).find(key => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
      if (stale) {
        const preset = JSON.parse(localStorage.getItem(key)!);
        preset.dataVersion = "vanilla:4:old:old";
        localStorage.setItem(key, JSON.stringify(preset));
      }
      return { key, value: localStorage.getItem(key) };
    }, stale);
    await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
    await page.evaluate(() => {
      Storage.prototype.setItem = () => { throw new DOMException("storage full", "QuotaExceededError"); };
    });
    await page.getByRole("button", { name: stale ? "Load inputs only" : "Load", exact: true }).click();
    await expect(page.getByRole("checkbox", { name: "Two-handing", exact: true })).not.toBeChecked();
    await expect(page.locator(".result-row-full")).toHaveCount(stale ? 0 : 1);
    await expect(page.getByText(/Comparison changes could not be saved/)).toBeVisible();
    expect(await page.evaluate(key => localStorage.getItem(key), stored.key)).toBe(stored.value);
  });
}

test("saving with result locks disabled keeps them disabled after loading", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.locator(".result-row-full").first().getByRole("button", { name: /^Lock / }).click();
  await page.getByText("Advanced", { exact: true }).click();
  const locks = page.getByRole("checkbox", { name: "Use Locked Result Stats", exact: true });
  await locks.uncheck();
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(locks).not.toBeChecked();
  const saved = await page.evaluate(() => {
    const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
    return JSON.parse(localStorage.getItem(key)!).request;
  });
  expect([saved.lockStr, saved.lockDex, saved.lockInt, saved.lockFai, saved.lockArc]).toEqual([null, null, null, null, null]);
});

test("a failed deletion reports the error and preserves a loadable build", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await page.evaluate(() => {
    const write = Storage.prototype.setItem;
    Object.assign(window, { restoreStorage: () => { Storage.prototype.setItem = write; } });
    Storage.prototype.setItem = function (key, value) {
      if (key === "tarnisheds-arsenal.savedBuildIndex.v1") throw new DOMException("index quota exceeded", "QuotaExceededError");
      write.call(this, key, value);
    };
  });
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await page.getByRole("button", { name: "Confirm Delete", exact: true }).click();
  await expect(page.locator('.error-strip[role="alert"]')).toContainText("index quota exceeded");
  await page.evaluate(() => (window as unknown as { restoreStorage: () => void }).restoreStorage());
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(page.getByText("Loaded Build Preset; saved results verified on current data.", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Confirm Delete", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "Saved", exact: true })).toHaveValue("");
});

test("Convergence saves, updates and reloads the displayed fixed stats", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("radio", { name: /Convergence/ }).click();
  await expect(page.getByText("Experimental fixed-stat model", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  await expect(page.getByText("Saved Build Preset.", { exact: true })).toBeVisible();
  await page.getByRole("spinbutton", { name: "STR", exact: true }).fill("40");
  await page.getByRole("button", { name: "Update selected", exact: true }).click();
  const total = Number(await page.getByRole("textbox", { name: "Stat total", exact: true }).inputValue());
  const saved = await page.evaluate(() => {
    const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
    return JSON.parse(localStorage.getItem(key)!);
  });
  expect(saved.request).toMatchObject({ profileId: "convergence", characterLevel: total, strStat: 40 });
  await page.reload();
  await page.getByRole("button", { name: "Load", exact: true }).click();
  await expect(page.getByRole("spinbutton", { name: "STR", exact: true })).toHaveValue("40");
  await expect(page.getByRole("textbox", { name: "Stat total", exact: true })).toHaveValue(String(total));
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("1 ranked rows")).toBeVisible();
});

for (const action of ["Save new", "Update selected"]) {
  test(`${action} discards solved rows after inputs change`, async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
    await page.locator(".result-row-full").first().getByRole("button", { name: /^Compare / }).click();
    if (action === "Update selected") await page.getByRole("button", { name: "Save new", exact: true }).click();
    await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
    await expect(page.getByText("Inputs changed", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: action, exact: true }).click();
    await expect(page.getByText(/Inputs only; rerun search for current results/)).toBeVisible();
    const saved = await page.evaluate(() => {
      const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
      return JSON.parse(localStorage.getItem(key)!);
    });
    expect(saved).toMatchObject({ request: { twoHanding: true }, selectedBuild: null, compareTarget: null, compareBench: [] });
    await page.getByRole("button", { name: "Load", exact: true }).click();
    await expect(page.locator(".result-row-full")).toHaveCount(0);
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
  });
}

for (const action of ["delete", "edit", "profile", "update"]) {
  test(`migration cannot overwrite a later ${action}`, async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Search", exact: true }).click();
    await expect(page.getByText("4 ranked rows")).toBeVisible();
    await page.getByRole("button", { name: "Save new", exact: true }).click();
    const original = await page.evaluate(() => {
      const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
      const preset = JSON.parse(localStorage.getItem(key)!);
      preset.dataVersion = "vanilla:4:old:old";
      localStorage.setItem(key, JSON.stringify(preset));
      return preset;
    });
    await page.reload();
    await page.evaluate(async () => {
      const modulePath = "/src/lib/api.ts";
      const { api } = await import(modulePath);
      const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
      const row = JSON.parse(localStorage.getItem(key)!).selectedBuild;
      api.solveBuild = (_base: unknown, _weapon: unknown, _affinity: unknown, _aow: unknown, signal: AbortSignal) => new Promise((resolve) => {
        Object.assign(window, { migrationSignal: signal, finishMigration: () => resolve(row) });
      });
    });
    await page.getByRole("button", { name: "Migrate data", exact: true }).click();
    await page.waitForFunction(() => "finishMigration" in window);
    if (action === "delete") {
      await page.getByRole("button", { name: "Delete", exact: true }).click();
      await page.getByRole("button", { name: "Confirm Delete", exact: true }).click();
    } else if (action === "profile") {
      await page.getByRole("radio", { name: /Convergence/ }).click();
      await expect(page.getByText("Experimental fixed-stat model", { exact: true })).toBeVisible();
    } else {
      await page.getByRole("checkbox", { name: "Two-handing", exact: true }).check();
      if (action === "update") await page.getByRole("button", { name: "Update selected", exact: true }).click();
    }
    await page.evaluate(async () => {
      if (!(window as unknown as { migrationSignal: AbortSignal }).migrationSignal.aborted) {
        throw new Error("Obsolete migration did not cancel its native request");
      }
      (window as unknown as { finishMigration: () => void }).finishMigration();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    const saved = await page.evaluate((id) => localStorage.getItem(`tarnisheds-arsenal.savedBuild.v2.${id}`), original.id);
    if (action === "delete") {
      expect(saved).toBeNull();
      await expect(page.getByRole("combobox", { name: "Saved", exact: true })).toHaveValue("");
    } else if (action === "update") {
      expect(JSON.parse(saved!)).toMatchObject({ request: { twoHanding: true }, selectedBuild: null });
    } else {
      expect(JSON.parse(saved!)).toEqual(original);
      if (action === "edit") await expect(page.getByRole("checkbox", { name: "Two-handing", exact: true })).toBeChecked();
      else await expect(page.getByRole("radio", { name: /Convergence/ })).toHaveAttribute("aria-checked", "true");
    }
    await expect(page.getByText(/Migrated Build Preset/)).toHaveCount(0);
  });
}

test("failed migration stops further recomputations and keeps the original error", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("4 ranked rows")).toBeVisible();
  await page.locator(".result-row-full").nth(0).getByRole("button", { name: /^Compare / }).click();
  await page.locator(".result-row-full").nth(1).getByRole("button", { name: /^Compare / }).click();
  await page.getByRole("button", { name: "Save new", exact: true }).click();
  const presetId = await page.evaluate(() => {
    const key = Object.keys(localStorage).find((key) => key.startsWith("tarnisheds-arsenal.savedBuild.v2."))!;
    const preset = JSON.parse(localStorage.getItem(key)!);
    preset.dataVersion = "vanilla:4:old:old";
    localStorage.setItem(key, JSON.stringify(preset));
    return preset.id;
  });
  await page.reload();
  await page.evaluate(async (id) => {
    const { api } = await import("/src/lib/api.ts");
    const preset = JSON.parse(localStorage.getItem(`tarnisheds-arsenal.savedBuild.v2.${id}`)!);
    const calls: Array<{ signal: AbortSignal }> = [];
    api.solveBuild = (_base: unknown, _weapon: unknown, _affinity: unknown, _aow: unknown, signal: AbortSignal) => {
      const call = calls.length;
      calls.push({ signal });
      if (call === 0) return Promise.resolve(preset.selectedBuild);
      if (call === 1) return Promise.reject(new Error("recompute failed"));
      return new Promise((_resolve, reject) => {
        signal.addEventListener("abort", () => {
          Object.assign(window, { migrationSiblingAborted: true });
          reject(new DOMException("Calculation stopped.", "AbortError"));
        }, { once: true });
      });
    };
    Object.assign(window, { migrationCalls: calls, migrationSiblingAborted: false });
  }, presetId);
  await page.getByRole("button", { name: "Migrate data", exact: true }).click();
  await expect(page.locator('.error-strip[role="alert"]')).toContainText("recompute failed");
  await expect.poll(() => page.evaluate(() => (window as unknown as { migrationCalls: Array<{ signal: AbortSignal }> }).migrationCalls.length)).toBe(2);
  expect(await page.evaluate(() => (window as unknown as { migrationCalls: Array<{ signal: AbortSignal }> }).migrationCalls.every(call => call.signal.aborted))).toBe(true);
});

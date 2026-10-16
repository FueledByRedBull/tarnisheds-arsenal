import { describe, expect, it, vi } from "vitest";
import { blockBrowserReload } from "./keyboard";

describe("blockBrowserReload", () => {
  it.each([
    { key: "F5", ctrlKey: false, blocked: true },
    { key: "F5", ctrlKey: true, blocked: true },
    { key: "r", ctrlKey: true, blocked: true },
    { key: "R", ctrlKey: true, blocked: true },
    { key: "p", ctrlKey: true, blocked: true },
    { key: "r", ctrlKey: false, blocked: false },
    { key: "k", ctrlKey: true, blocked: false },
    { key: "1", ctrlKey: true, blocked: false },
  ])("$key with ctrl $ctrlKey: blocked $blocked", ({ key, ctrlKey, blocked }) => {
    const preventDefault = vi.fn();
    blockBrowserReload({ key, ctrlKey, metaKey: false, preventDefault });
    expect(preventDefault).toHaveBeenCalledTimes(blocked ? 1 : 0);
  });
});

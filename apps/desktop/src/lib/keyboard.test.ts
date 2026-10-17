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
    // Non-Latin layouts report the typed letter; the physical R and P keys still reload and print.
    { key: "\u043a", code: "KeyR", ctrlKey: true, blocked: true },
    { key: "\u03c0", code: "KeyP", ctrlKey: true, blocked: true },
    { key: "\u043a", code: "KeyR", ctrlKey: false, blocked: false },
  ])("$key with ctrl $ctrlKey: blocked $blocked", ({ key, code = "", ctrlKey, blocked }) => {
    const preventDefault = vi.fn();
    blockBrowserReload({ key, code, ctrlKey, metaKey: false, preventDefault });
    expect(preventDefault).toHaveBeenCalledTimes(blocked ? 1 : 0);
  });
});

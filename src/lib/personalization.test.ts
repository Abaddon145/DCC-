import { describe, expect, it } from "vitest";
import { normalizeShortcut, shortcutConflicts, textColorFor } from "./personalization";

describe("personalization", () => {
  it("finds duplicate shortcuts", () => expect([...shortcutConflicts({ a: "Ctrl+P", b: "ctrl+p" })].sort()).toEqual(["a", "b"]));
  it("captures a shortcut", () => expect(normalizeShortcut(new KeyboardEvent("keydown", { key: "n", ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+N"));
  it("chooses readable accent text", () => { expect(textColorFor("#FFFFFF")).toBe("#111111"); expect(textColorFor("#101010")).toBe("#FFFFFF"); });
});

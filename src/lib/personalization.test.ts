import { describe, expect, it } from "vitest";
import { moduleRegistry, normalizeShortcut, shortcutConflicts, textColorFor } from "./personalization";

describe("personalization", () => {
  it("finds duplicate shortcuts", () => expect([...shortcutConflicts({ a: "Ctrl+P", b: "ctrl+p" })].sort()).toEqual(["a", "b"]));
  it("captures a shortcut", () => expect(normalizeShortcut(new KeyboardEvent("keydown", { key: "n", ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+N"));
  it("chooses readable accent text", () => { expect(textColorFor("#FFFFFF")).toBe("#111111"); expect(textColorFor("#101010")).toBe("#FFFFFF"); });
  it("uses four independent media modules and removes manual collections", () => {
    expect(moduleRegistry.slice(0, 6).map(item => item.id)).toEqual(["library", "imageLibrary", "modelLibrary", "audioLibrary", "videoLibrary", "projects"]);
    expect(moduleRegistry.some(item => item.id === ("collections" as never))).toBe(false);
  });
  it("keeps navigation modules in stable product groups", () => {
    expect(moduleRegistry.filter(item => item.group === "content").map(item => item.id)).toEqual(["library", "imageLibrary", "modelLibrary", "audioLibrary", "videoLibrary"]);
    expect(moduleRegistry.filter(item => item.group === "libraryContext").map(item => item.id)).toEqual(["smartCollections", "favorites", "recent"]);
    expect(moduleRegistry.filter(item => item.group === "manage").map(item => item.id)).toEqual(["tagManager", "health", "trash"]);
  });
});

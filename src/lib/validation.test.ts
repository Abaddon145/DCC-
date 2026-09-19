import { describe, expect, it } from "vitest";
import { parseSize, splitValues, validateAsset } from "./validation";
import type { AssetInput } from "../types";

const base: AssetInput = {
  name: "古堡环境", description: "", categoryId: null, tags: [], dccTools: ["Unreal Engine"],
  versions: ["5.5"], formats: ["uasset"], sizeBytes: null, author: "", sourceUrl: "",
  license: "", shareUrl: "https://pan.baidu.com/s/demo", extractionCode: "a1b2", favorite: false, images: [], contentLanguage: "zh-CN",
  localizations: { "zh-CN": { name: "古堡环境", description: "", tags: [], license: "" }, en: { name: "", description: "", tags: [], license: "" } }
};

describe("asset validation", () => {
  it("accepts a normal Baidu entry", () => expect(validateAsset(base)).toEqual({ errors: {}, warnings: [] }));
  it("rejects unsafe protocols", () => expect(validateAsset({ ...base, shareUrl: "file:///tmp/a" }).errors.shareUrl).toBeTruthy());
  it("warns for a non-Baidu link", () => expect(validateAsset({ ...base, shareUrl: "https://example.com/a" }).warnings).toHaveLength(1));
  it("normalizes separated values", () => expect(splitValues("UE;ue, Nanite；Nanite")).toEqual(["UE", "Nanite"]));
  it("parses readable sizes", () => expect(parseSize("1.5 GB")).toBe(1610612736));
});

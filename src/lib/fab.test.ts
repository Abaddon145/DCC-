import { describe, expect, it } from "vitest";
import type { AssetInput, FabMetadata } from "../types";
import { applyFabMetadata, isFabUrl } from "./fab";

const asset: AssetInput = {
  name: "", description: "本地说明", categoryId: null, tags: ["Environment"], dccTools: ["Unreal Engine"],
  versions: [], formats: [], sizeBytes: null, author: "", sourceUrl: "", license: "", shareUrl: "",
  extractionCode: "", favorite: false, images: [], contentLanguage: "zh-CN",
  localizations: { "zh-CN": { name: "", description: "本地说明", tags: ["Environment"], license: "" }, en: { name: "", description: "", tags: [], license: "" } }
};
const metadata: FabMetadata = {
  canonicalUrl: "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a", name: "Garden",
  listingId: "06003f78-9a59-4fb8-abbc-14dc276f0b4a", categoryPath: "environments/garden", listingType: "3d-model", suggestedCategoryPath: ["环境", "花园"],
  description: "Fab description", author: "Studio", category: "Environment", tags: ["environment", "Garden"],
  dccTools: ["Unreal Engine"], versions: ["5.4"], formats: ["uasset"], license: "Fab Standard License",
  previewImages: [{ sourcePath: "C:\\Temp\\fab.jpg", previewDataUrl: "data:image/jpeg;base64,AA==", originalName: "fab.jpg", remoteUrl: "https://media.fab.com/fab.jpg" }], imageWarning: null
};

describe("Fab metadata", () => {
  it("fills empty fields, preserves edited text, and merges values", () => {
    const result = applyFabMetadata(asset, metadata);
    expect(result.name).toBe("Garden");
    expect(result.description).toBe("本地说明");
    expect(result.author).toBe("Studio");
    expect(result.tags).toEqual(["Environment", "Garden"]);
    expect(result.versions).toEqual(["5.4"]);
    expect(result.sourceUrl).toBe(metadata.canonicalUrl);
    expect(result.fabListingId).toBe(metadata.listingId);
    expect(result.autoCategoryPath).toEqual(["环境", "花园"]);
    expect(result.images).toHaveLength(1);
    expect(result.images[0].isCover).toBe(true);
  });
  it("recognizes Fab listing links", () => {
    expect(isFabUrl(metadata.canonicalUrl)).toBe(true);
    expect(isFabUrl("https://example.com/listings/id")).toBe(false);
  });
  it("keeps the source URL and listing ID aligned when explicitly reading another Fab product", () => {
    const result = applyFabMetadata({ ...asset, sourceUrl: "https://www.fab.com/listings/11111111-1111-1111-1111-111111111111", fabListingId: "11111111-1111-1111-1111-111111111111" }, metadata);
    expect(result.sourceUrl).toBe(metadata.canonicalUrl);
    expect(result.fabListingId).toBe(metadata.listingId);
  });
  it("places mixed Fab source fields into the matching language and keeps manual classifications", () => {
    const result = applyFabMetadata({ ...asset, categoryId: "manual" }, { ...metadata, name: "中文庭院", description: "Quiet courtyard", tags: ["庭院", "Garden"] });
    expect(result.localizations["zh-CN"]?.name).toBe("中文庭院");
    expect(result.localizations["zh-CN"]?.description).toBe("本地说明");
    expect(result.localizations.en?.name).toBe("");
    expect(result.localizations.en?.description).toBe("Quiet courtyard");
    expect(result.categoryId).toBe("manual");
    expect(result.autoCategoryPath).toEqual([]);
    expect(applyFabMetadata({ ...asset, id: "existing" }, metadata).autoCategoryPath).toEqual(["环境", "花园"]);
    expect(applyFabMetadata({ ...asset, id: "existing", categoryId: "manual" }, metadata).autoCategoryPath).toEqual([]);
  });
});

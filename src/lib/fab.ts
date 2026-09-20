import type { AssetInput, FabMetadata } from "../types";

function mergeUnique(current: string[], incoming: string[]) {
  const values = [...current];
  const seen = new Set(current.map(value => value.trim().toLocaleLowerCase("zh-CN")));
  incoming.forEach(value => {
    const trimmed = value.trim();
    const key = trimmed.toLocaleLowerCase("zh-CN");
    if (trimmed && !seen.has(key)) { seen.add(key); values.push(trimmed); }
  });
  return values;
}

export function applyFabMetadata(input: AssetInput, metadata: FabMetadata): AssetInput {
  const remoteUrls = new Set(input.images.map(image => image.remoteUrl).filter(Boolean));
  const incomingImages = metadata.previewImages
    .filter(image => !remoteUrls.has(image.remoteUrl))
    .map((image, index) => ({
      sourcePath: image.sourcePath,
      previewDataUrl: image.previewDataUrl,
      originalName: image.originalName,
      remoteUrl: image.remoteUrl,
      isCover: input.images.length === 0 && index === 0,
      sortOrder: input.images.length + index
    }));
  const english = input.localizations.en || { name: "", description: "", tags: [], license: "" };
  const nextEnglish = {
    name: english.name.trim() ? english.name : metadata.name,
    description: english.description.trim() ? english.description : metadata.description,
    license: english.license.trim() ? english.license : metadata.license,
    tags: mergeUnique(english.tags, metadata.tags)
  };
  return {
    ...input,
    name: input.name.trim() ? input.name : metadata.name,
    description: input.description.trim() ? input.description : metadata.description,
    author: input.author.trim() ? input.author : metadata.author,
    sourceUrl: input.sourceUrl.trim() ? input.sourceUrl : metadata.canonicalUrl,
    fabListingId: metadata.listingId,
    autoCategoryPath: input.categoryId ? [] : metadata.suggestedCategoryPath,
    license: input.license.trim() ? input.license : metadata.license,
    tags: mergeUnique(input.tags, metadata.tags),
    dccTools: mergeUnique(input.dccTools, metadata.dccTools),
    versions: mergeUnique(input.versions, metadata.versions),
    formats: mergeUnique(input.formats, metadata.formats),
    images: [...input.images, ...incomingImages]
    ,localizations: { ...input.localizations, en: nextEnglish }
  };
}

export function isFabUrl(value: string) {
  try {
    const url = new URL(value);
    return (url.hostname === "fab.com" || url.hostname === "www.fab.com") && url.pathname.includes("/listings/");
  } catch { return false; }
}

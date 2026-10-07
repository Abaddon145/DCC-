import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AssetEditor } from "./AssetEditor";
import { api } from "../lib/api";
import type { FabMetadata } from "../types";

vi.mock("../lib/api", () => ({ api: {
  translateAssetFields: vi.fn(), getTranslationSettings: vi.fn(), fetchFabMetadata: vi.fn(),
  parseShareText: vi.fn(), readClipboard: vi.fn(), checkShareUrl: vi.fn(), checkFabUrl: vi.fn()
} }));
vi.mock("./ImagePreview", () => ({ ImagePreview: () => <div /> }));

describe("AssetEditor online translation", () => {
  beforeEach(() => vi.clearAllMocks());

  it("previews translated fields and only writes them after confirmation", async () => {
    vi.mocked(api.translateAssetFields).mockResolvedValue({
      fields: { name: "Forest Ruins", description: "Environment pack", tags: ["Environment"], license: "Commercial" },
      characterCount: 12, warnings: [], failedFields: [], appliedTerms: [], protectedTokenCount: 0
    });
    render(<AssetEditor asset={null} categories={[]} contentLanguage="zh-CN" onClose={vi.fn()} onSave={vi.fn()} />);
    fireEvent.change(screen.getByLabelText(/中文名称/), { target: { value: "森林遗迹" } });
    fireEvent.change(screen.getByLabelText(/中文描述/), { target: { value: "环境包" } });
    fireEvent.click(screen.getByRole("button", { name: /Translate to English/ }));
    await waitFor(() => expect(screen.getByText(/翻译预览/)).toBeInTheDocument());
    expect(screen.queryByDisplayValue("Forest Ruins")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "应用所选译文" }));
    expect(screen.getByDisplayValue("Forest Ruins")).toBeInTheDocument();
    expect(api.translateAssetFields).toHaveBeenCalledWith(expect.objectContaining({ sourceLanguage: "zh-CN", targetLanguage: "en" }));
  });

  it("stops before fetching Fab metadata when the listing already exists", async () => {
    vi.mocked(api.checkFabUrl).mockResolvedValue({ assetId: "asset-1", assetName: "沙漠沙丘", categoryPath: "环境 / 沙漠", location: "library" });
    const onOpenExisting = vi.fn();
    render(<AssetEditor asset={null} categories={[]} contentLanguage="zh-CN" onClose={vi.fn()} onSave={vi.fn()} onOpenExisting={onOpenExisting} />);
    fireEvent.change(screen.getByPlaceholderText("https://www.fab.com/listings/…"), { target: { value: "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a?lang=zh-CN" } });
    fireEvent.click(screen.getByRole("button", { name: "读取并填充" }));
    await waitFor(() => expect(screen.getByText(/已存在于/)).toBeInTheDocument());
    expect(api.fetchFabMetadata).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "打开现有素材" }));
    expect(onOpenExisting).toHaveBeenCalledWith("asset-1");
  });

  it("keeps text and category edited during Fab reading and defers automatic translation until save", async () => {
    vi.mocked(api.checkFabUrl).mockResolvedValue(null);
    let finish!: (metadata: FabMetadata) => void;
    vi.mocked(api.fetchFabMetadata).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<AssetEditor asset={null} categories={[{ id: "manual", name: "手选分类", parentId: null, sortOrder: 0, assetCount: 0 }]} contentLanguage="zh-CN" onClose={vi.fn()} onSave={onSave} />);
    fireEvent.change(screen.getByPlaceholderText("https://www.fab.com/listings/…"), { target: { value: "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a" } });
    fireEvent.click(screen.getByRole("button", { name: "读取并填充" }));
    await waitFor(() => expect(finish).toBeDefined());
    fireEvent.change(screen.getByLabelText(/中文名称/), { target: { value: "等待时手填名称" } });
    fireEvent.change(screen.getByLabelText("分类"), { target: { value: "manual" } });
    finish({ canonicalUrl: "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a", listingId: "06003f78-9a59-4fb8-abbc-14dc276f0b4a", categoryPath: "environments/courtyard", listingType: "3d-model", suggestedCategoryPath: ["环境", "庭院"], name: "Quiet courtyard", description: "", author: "", category: "", tags: [], dccTools: [], versions: [], formats: [], license: "", previewImages: [], imageWarning: null });
    await waitFor(() => expect(screen.getByText(/保存时自动补齐缺失的中英版本/)).toBeInTheDocument());
    expect(screen.getByDisplayValue("等待时手填名称")).toBeInTheDocument();
    expect(screen.getByLabelText("分类")).toHaveValue("manual");
    expect(api.translateAssetFields).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "保存素材" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ categoryId: "manual", autoCategoryPath: [], localizations: expect.objectContaining({ "zh-CN": expect.objectContaining({ name: "等待时手填名称" }) }) })));
  });

  it("starts one save and freezes fields while bilingual completion is pending", async () => {
    let finish!: () => void;
    const onSave = vi.fn().mockImplementation(() => new Promise<void>(resolve => { finish = resolve; }));
    render(<AssetEditor asset={null} categories={[]} contentLanguage="zh-CN" onClose={vi.fn()} onSave={onSave} />);
    fireEvent.change(screen.getByLabelText(/中文名称/), { target: { value: "保存中的素材" } });
    const button = screen.getByRole("button", { name: "保存素材" });
    fireEvent.click(button);
    fireEvent.click(button);
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(screen.getByLabelText(/中文名称/)).toBeDisabled();
    expect(screen.getByRole("button", { name: /正在保存并补齐双语/ })).toBeDisabled();
    await act(async () => finish());
  });

  it("clears previous Fab identity and suggested category when the source is manually changed", async () => {
    vi.mocked(api.checkFabUrl).mockResolvedValue(null);
    const source = "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a";
    vi.mocked(api.fetchFabMetadata).mockResolvedValue({ canonicalUrl: source, listingId: "06003f78-9a59-4fb8-abbc-14dc276f0b4a", categoryPath: "environments/courtyard", listingType: "3d-model", suggestedCategoryPath: ["环境", "庭院"], name: "Quiet courtyard", description: "", author: "", category: "", tags: [], dccTools: [], versions: [], formats: [], license: "", previewImages: [], imageWarning: null });
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<AssetEditor asset={null} categories={[]} contentLanguage="en" onClose={vi.fn()} onSave={onSave} />);
    fireEvent.change(screen.getByPlaceholderText("https://www.fab.com/listings/…"), { target: { value: source } });
    fireEvent.click(screen.getByRole("button", { name: "读取并填充" }));
    await waitFor(() => expect(screen.getByText(/保存时自动归类/)).toBeInTheDocument());
    fireEvent.change(screen.getByLabelText("来源地址"), { target: { value: "https://example.com/custom" } });
    expect(screen.queryByText(/保存时自动归类/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "保存素材" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ sourceUrl: "https://example.com/custom", fabListingId: null, autoCategoryPath: [] })));
  });
});

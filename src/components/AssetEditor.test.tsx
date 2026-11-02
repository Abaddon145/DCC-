import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AssetEditor as ActualAssetEditor } from "./AssetEditor";
import { useLayoutEffect, type ComponentProps } from "react";
import { api } from "../lib/api";
import type { AssetDetail, FabMetadata } from "../types";

vi.mock("../lib/api", () => ({ api: {
  translateAssetFields: vi.fn(), getTranslationSettings: vi.fn(), fetchFabMetadata: vi.fn(),
  parseShareText: vi.fn(), readClipboard: vi.fn(), checkShareUrl: vi.fn(), checkFabUrl: vi.fn(), setFabAutoTranslate: vi.fn()
  , estimateTranslationCost: vi.fn()
} }));
vi.mock("./ImagePreview", () => ({ ImagePreview: () => <div /> }));
const AssetEditor = (props: ComponentProps<typeof ActualAssetEditor>) => <ActualAssetEditor expectedLibraryId="test-library" {...props} />;
const confirmCost = async () => fireEvent.click(await screen.findByRole("button", { name: "确认并补译" }));

describe("AssetEditor online translation", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.getTranslationSettings).mockResolvedValue({ provider: "deepseek", configured: true, fabAutoTranslate: true, contentLanguage: "zh-CN" });
    vi.mocked(api.estimateTranslationCost).mockResolvedValue({ pendingFieldCount: 2, characters: 30, cacheHits: 1, tokensMin: 0, tokensMax: 6000, month: "2026-10", monthUsed: 500, monthLimit: 1000000, wouldExceed: false, onlineRequests: 1, baiduCharacters: 0, rows: [], warnings: [] });
  });

  it("retains a new confirmation started before the changed fingerprint effect finishes", async () => {
    function ImmediateSave({ libraryId, submit }: { libraryId: string; submit: boolean }) {
      useLayoutEffect(() => {
        if (submit) screen.getByRole("button", { name: "保存素材" }).click();
      }, [submit]);
      return <AssetEditor asset={null} categories={[]} contentLanguage="zh-CN" expectedLibraryId={libraryId} onClose={vi.fn()} onSave={vi.fn()} />;
    }
    const { rerender } = render(<ImmediateSave libraryId="first" submit={false} />);
    fireEvent.change(screen.getByLabelText(/中文名称/), { target: { value: "新素材" } });
    await waitFor(() => expect(screen.getByRole("button", { name: "保存素材" })).toBeEnabled());
    rerender(<ImmediateSave libraryId="second" submit />);
    expect(await screen.findByRole("dialog", { name: "翻译费用确认" })).toBeInTheDocument();
    expect(await screen.findByText(/0–6,000/)).toBeInTheDocument();
    expect(api.estimateTranslationCost).toHaveBeenCalledWith(expect.anything(), "second");
  });

  it("shows_pending_fields_cache_and_reserved_interval", async () => {
    const onSave = vi.fn();
    render(<AssetEditor asset={null} categories={[]} contentLanguage="zh-CN" onClose={vi.fn()} onSave={onSave} />);
    fireEvent.change(screen.getByLabelText(/中文名称/), { target: { value: "新素材" } });
    await waitFor(() => expect(screen.getByRole("button", { name: "保存素材" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "保存素材" }));
    expect(await screen.findByRole("dialog", { name: "翻译费用确认" })).toBeInTheDocument();
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText(/0–6,000/)).toBeInTheDocument();
    expect(screen.getByText(/缓存命中 1/)).toBeInTheDocument();
  });

  it("ignores_stale_estimate_on_input_or_library_change", async () => {
    let finish!: (value: Awaited<ReturnType<typeof api.estimateTranslationCost>>) => void;
    vi.mocked(api.estimateTranslationCost).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    const callbacks = { asset: null, categories: [], contentLanguage: "zh-CN" as const, onClose: vi.fn(), onSave: vi.fn() };
    const { rerender } = render(<AssetEditor {...callbacks} expectedLibraryId="first" />);
    fireEvent.change(screen.getByLabelText(/中文名称/), { target: { value: "旧输入" } });
    await waitFor(() => expect(screen.getByRole("button", { name: "保存素材" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "保存素材" }));
    await waitFor(() => expect(finish).toBeDefined());
    rerender(<AssetEditor {...callbacks} expectedLibraryId="second" />);
    await act(async () => finish({ pendingFieldCount: 1, characters: 3, cacheHits: 0, tokensMin: 0, tokensMax: 5000, month: "2026-10", monthUsed: 0, monthLimit: 1e6, wouldExceed: false, onlineRequests: 1, baiduCharacters: 0, rows: [], warnings: [] }));
    expect(screen.queryByRole("dialog", { name: "翻译费用确认" })).not.toBeInTheDocument();
    expect(callbacks.onSave).not.toHaveBeenCalled();
    expect(api.estimateTranslationCost).toHaveBeenCalledWith(expect.objectContaining({ form: expect.objectContaining({ localizations: expect.objectContaining({ "zh-CN": expect.objectContaining({ name: "旧输入" }) }) }) }), "first");
  });

  it("keeps_unread_fab_out_of_online_estimates_and_can_save_raw", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<AssetEditor asset={null} categories={[]} contentLanguage="zh-CN" onClose={vi.fn()} onSave={onSave} />);
    fireEvent.change(screen.getByLabelText(/中文名称/), { target: { value: "尚未读取" } });
    fireEvent.change(screen.getByLabelText("来源地址"), { target: { value: "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a" } });
    await waitFor(() => expect(screen.getByRole("button", { name: "保存素材" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "保存素材" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("请先点击“读取并填充”");
    expect(api.estimateTranslationCost).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "仅保存原文" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.anything(), expect.objectContaining({ skipAutoTranslation: true })));
  });

  it("shows the real disabled translation policy instead of promising automatic Chinese", async () => {
    vi.mocked(api.getTranslationSettings).mockResolvedValue({ provider: "deepseek", configured: true, fabAutoTranslate: false, contentLanguage: "zh-CN" });
    render(<AssetEditor asset={null} categories={[]} contentLanguage="zh-CN" onClose={vi.fn()} onSave={vi.fn()} />);
    expect(await screen.findByText(/自动补译已关闭/)).toBeInTheDocument();
  });

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
    await screen.findByText(/已读取“Quiet courtyard”/);
    expect(screen.getByDisplayValue("等待时手填名称")).toBeInTheDocument();
    expect(screen.getByLabelText("分类")).toHaveValue("manual");
    expect(api.translateAssetFields).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "保存素材" }));
    await confirmCost();
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ categoryId: "manual", autoCategoryPath: [], localizations: expect.objectContaining({ "zh-CN": expect.objectContaining({ name: "等待时手填名称" }) }) }), { completeBilingual: true, autoTranslate: true }));
  });

  it("starts one save and freezes fields while bilingual completion is pending", async () => {
    let finish!: () => void;
    const onSave = vi.fn().mockImplementation(() => new Promise<void>(resolve => { finish = resolve; }));
    render(<AssetEditor asset={null} categories={[]} contentLanguage="zh-CN" onClose={vi.fn()} onSave={onSave} />);
    fireEvent.change(screen.getByLabelText(/中文名称/), { target: { value: "保存中的素材" } });
    const button = screen.getByRole("button", { name: "保存素材" });
    await waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button);
    fireEvent.click(button);
    await confirmCost();
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(screen.getByLabelText(/中文名称/)).toBeDisabled();
    expect(screen.getByRole("button", { name: /正在保存并补齐双语/ })).toBeDisabled();
    await act(async () => finish());
  });

  it("clears previous Fab identity and suggested category when the source is manually changed", async () => {
    vi.mocked(api.checkFabUrl).mockResolvedValue(null);
    const source = "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a";
    vi.mocked(api.fetchFabMetadata).mockResolvedValue({ canonicalUrl: source, listingId: "06003f78-9a59-4fb8-abbc-14dc276f0b4a", categoryPath: "environments/courtyard", listingType: "3d-model", suggestedCategoryPath: ["环境", "庭院"], name: "Quiet courtyard", description: "", author: "", category: "", tags: [], dccTools: [], versions: [], formats: [], license: "", previewImages: [], imageWarning: null, formatWarning: "该商品还包含非 UE 格式，已忽略：fbx" });
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<AssetEditor asset={null} categories={[]} contentLanguage="en" onClose={vi.fn()} onSave={onSave} />);
    fireEvent.change(screen.getByPlaceholderText("https://www.fab.com/listings/…"), { target: { value: source } });
    fireEvent.click(screen.getByRole("button", { name: "读取并填充" }));
    await waitFor(() => expect(screen.getByText(/保存时自动归类/)).toBeInTheDocument());
    fireEvent.change(screen.getByLabelText("来源地址"), { target: { value: "https://example.com/custom" } });
    expect(screen.queryByText(/保存时自动归类/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "保存素材" }));
    await confirmCost();
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ sourceUrl: "https://example.com/custom", fabListingId: null, autoCategoryPath: [] }), { completeBilingual: false, autoTranslate: true }));
  });

  it("retains a saved result and retries only the missing fields without creating another asset", async () => {
    const saved: AssetDetail = {
      id: "saved-1", name: "手填标题", description: "", categoryId: "courtyard", categoryName: "庭院", tags: [], dccTools: [], versions: [], formats: [], favorite: false,
      updatedAt: "2026-10-08", createdAt: "2026-10-08", lastViewedAt: null, coverImageId: null, contentLanguage: "zh-CN", languageFallback: false,
      linkCheckStatus: "unknown", linkCheckedAt: null, linkCheckMessage: "", sizeBytes: null, author: "", sourceUrl: "", fabListingId: "", license: "", shareUrl: "", extractionCode: "", images: [],
      localizations: { "zh-CN": { name: "手填标题", description: "", tags: [], license: "" }, en: { name: "Quiet courtyard", description: "Environment", tags: [], license: "" } },
      singleImportWarnings: ["翻译额度不足，已保留原文"]
    };
    const onSave = vi.fn().mockResolvedValue(saved), onClose = vi.fn();
    const onRetryTranslation = vi.fn().mockResolvedValue({ ...saved, localizations: { ...saved.localizations, "zh-CN": { name: "手填标题", description: "环境", tags: [], license: "" } }, singleImportWarnings: [] });
    render(<AssetEditor asset={null} categories={[]} contentLanguage="zh-CN" onClose={onClose} onSave={onSave} onRetryTranslation={onRetryTranslation} />);
    fireEvent.change(screen.getByLabelText(/中文名称/), { target: { value: "手填标题" } });
    await waitFor(() => expect(screen.getByRole("button", { name: "保存素材" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "保存素材" }));
    await confirmCost();
    expect(await screen.findByText("待补译：中文描述")).toBeInTheDocument();
    expect(screen.getByText("翻译额度不足，已保留原文")).toBeInTheDocument();
    expect(screen.getByText("分类：庭院")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "重试缺失译文" }));
    await confirmCost();
    await waitFor(() => expect(onRetryTranslation).toHaveBeenCalledWith("saved-1"));
    expect(await screen.findByText("已有原文对应的中英字段已齐全。")).toBeInTheDocument();
    expect(screen.getByDisplayValue("手填标题")).toBeInTheDocument();
    expect(onSave).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "完成" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("keeps an unsaved draft when closing is declined", async () => {
    const confirmation = vi.spyOn(window, "confirm").mockReturnValue(false), onClose = vi.fn();
    render(<AssetEditor asset={null} categories={[]} contentLanguage="zh-CN" onClose={onClose} onSave={vi.fn()} />);
    fireEvent.change(screen.getByLabelText(/中文名称/), { target: { value: "未保存草稿" } });
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByDisplayValue("未保存草稿")).toBeInTheDocument();
    confirmation.mockRestore();
  });

  it.each(["successful read", "failed reread", "unapplied read URL"])("keeps the applied format notice after %s and passes it with save, not asset data", async mode => {
    const warning = "该商品还包含非 UE 格式，已忽略：blender、fbx";
    const source = "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a";
    vi.mocked(api.checkFabUrl).mockResolvedValue(null);
    vi.mocked(api.fetchFabMetadata).mockResolvedValue({ canonicalUrl: source, listingId: "06003f78-9a59-4fb8-abbc-14dc276f0b4a", categoryPath: "environments", listingType: "3d-model", suggestedCategoryPath: ["环境"], name: "UE pack", description: "", author: "", category: "", tags: [], dccTools: ["Unreal Engine"], versions: [], formats: ["uasset"], license: "", previewImages: [], imageWarning: null, formatWarning: warning });
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<AssetEditor asset={null} categories={[]} contentLanguage="en" onClose={vi.fn()} onSave={onSave} />);
    fireEvent.change(screen.getByPlaceholderText("https://www.fab.com/listings/…"), { target: { value: source } });
    fireEvent.click(screen.getByRole("button", { name: "读取并填充" }));
    expect(await screen.findByText(new RegExp(warning))).toBeInTheDocument();
    expect(onSave).not.toHaveBeenCalled();
    if (mode === "failed reread") {
      vi.mocked(api.fetchFabMetadata).mockRejectedValueOnce(new Error("商品读取失败"));
      await waitFor(() => expect(screen.getByRole("button", { name: "读取并填充" })).toBeEnabled());
      fireEvent.click(screen.getByRole("button", { name: "读取并填充" }));
      expect(await screen.findByText(/商品读取失败/)).toBeInTheDocument();
    } else if (mode === "unapplied read URL") {
      fireEvent.change(screen.getByPlaceholderText("https://www.fab.com/listings/…"), { target: { value: "https://www.fab.com/listings/95ff3c25-59f9-441d-aa43-93c3abd63680" } });
    }
    await waitFor(() => expect(screen.getByRole("button", { name: "保存素材" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "保存素材" }));
    await confirmCost();
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.not.objectContaining({ formatWarning: expect.anything() }), expect.objectContaining({ formatWarning: warning })));
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ sourceUrl: source, fabListingId: "06003f78-9a59-4fb8-abbc-14dc276f0b4a" }), expect.anything());
  });
});

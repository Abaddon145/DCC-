import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DetailPanel } from "./DetailPanel";
import type { AssetDetail } from "../types";

vi.mock("./ImagePreview", () => ({ ImagePreview: ({ alt }: { alt: string }) => <img alt={alt} /> }));
vi.mock("./ImageLightbox", () => ({ ImageLightbox: () => null }));
vi.mock("./EpicFabDialog", () => ({ EpicFabDialog: ({ onClose, productName }: { onClose: () => void; productName: string }) => <div role="dialog" aria-label="通过 Epic 添加到 UE 工程"><span>{productName}</span><button onClick={onClose}>关闭 Epic 添加窗口</button></div> }));

const asset: AssetDetail = {
  id: "1", name: "中文素材", categoryName: null, tags: ["环境"], dccTools: ["Unreal Engine"], versions: ["5.5"], formats: ["uasset"], favorite: false,
  updatedAt: "2026-01-01T00:00:00Z", lastViewedAt: null, coverImageId: null, contentLanguage: "zh-CN", languageFallback: false,
  linkCheckStatus: "unknown", linkCheckedAt: null, linkCheckMessage: "",
  description: "中文描述", categoryId: null, sizeBytes: null, author: "Studio", sourceUrl: "", fabListingId: "", license: "中文许可", shareUrl: "https://pan.baidu.com/s/demo", extractionCode: "", images: [], createdAt: "2026-01-01T00:00:00Z",
  localizations: {
    "zh-CN": { name: "中文素材", description: "中文描述", tags: ["环境"], license: "中文许可" },
    en: { name: "English Asset", description: "English description", tags: ["Environment"], license: "English license" }
  }
};

const handlers = { onClose: vi.fn(), onEdit: vi.fn(), onDelete: vi.fn(), onFavorite: vi.fn(), onOpen: vi.fn(), onSourceOpen: vi.fn(), onCopy: vi.fn(), onCheckLink: vi.fn(), checkingLink: false };

describe("DetailPanel bilingual content", () => {
  it("groups source actions separately from management actions", () => {
    render(<DetailPanel asset={asset} contentLanguage="zh-CN" loading={false} {...handlers} />);
    expect(screen.getByRole("group", { name: "打开素材" })).toContainElement(screen.getByRole("button", { name: "打开百度网盘" }));
    expect(screen.getByRole("group", { name: "管理素材" })).toContainElement(screen.getByRole("button", { name: "编辑" }));
  });
  it("shows key properties before the long description", () => {
    render(<DetailPanel asset={asset} contentLanguage="zh-CN" loading={false} {...handlers} />);
    expect(screen.getByText("软件 / 版本").compareDocumentPosition(screen.getByText("中文描述")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
  it("keeps long descriptions collapsed until requested", () => {
    const description = "长素材介绍".repeat(100);
    render(<DetailPanel asset={{ ...asset, localizations: { ...asset.localizations, "zh-CN": { ...asset.localizations["zh-CN"]!, description } } }} contentLanguage="zh-CN" loading={false} {...handlers} />);
    expect(screen.getByRole("button", { name: "展开完整介绍" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "展开完整介绍" }));
    expect(screen.getByText(description)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "收起介绍" })).toBeInTheDocument();
  });
  it("reports a missing Chinese description even when the Chinese title exists and retries that asset", () => {
    const onTranslate = vi.fn();
    render(<DetailPanel asset={{ ...asset, localizations: { ...asset.localizations, "zh-CN": { ...asset.localizations["zh-CN"]!, description: "" } } }} contentLanguage="zh-CN" loading={false} {...handlers} onTranslate={onTranslate} />);
    expect(screen.getByText(/中文描述/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "补齐缺失双语" }));
    expect(onTranslate).toHaveBeenCalledWith("1");
  });
  it("offers Epic for Fab assets without requiring a local file and keeps detail open on modal Escape", () => {
    const onClose = vi.fn();
    render(<DetailPanel asset={{ ...asset, fabListingId: "06003f78-9a59-4fb8-abbc-14dc276f0b4a", sourceUrl: "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a" }} contentLanguage="zh-CN" loading={false} {...handlers} onClose={onClose}/>);
    fireEvent.click(screen.getByRole("button", { name: "通过 Epic 添加到 UE 工程" }));
    expect(screen.getByRole("dialog", { name: "通过 Epic 添加到 UE 工程" })).toBeInTheDocument();
    expect(screen.getByText("English Asset")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "关闭 Epic 添加窗口" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("opens the Fab dialog for a Chinese-only asset using its visible name as fallback", () => {
    render(<DetailPanel asset={{ ...asset, fabListingId: "06003f78-9a59-4fb8-abbc-14dc276f0b4a", sourceUrl: "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a", localizations: { "zh-CN": asset.localizations["zh-CN"] } }} contentLanguage="zh-CN" loading={false} {...handlers}/>);
    fireEvent.click(screen.getByRole("button", { name: "通过 Epic 添加到 UE 工程" }));
    expect(screen.getByRole("dialog", { name: "通过 Epic 添加到 UE 工程" })).toHaveTextContent("中文素材");
  });
  it("temporarily switches detail content without changing the global preference", () => {
    render(<DetailPanel asset={asset} contentLanguage="zh-CN" loading={false} {...handlers} />);
    expect(screen.getByRole("heading", { name: "中文素材" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "English" }));
    expect(screen.getByRole("heading", { name: "English Asset" })).toBeInTheDocument();
    expect(screen.getByText("English description")).toBeInTheDocument();
  });

  it("marks and falls back when the requested language has no name", () => {
    const withoutEnglish = { ...asset, localizations: { "zh-CN": asset.localizations["zh-CN"] } };
    render(<DetailPanel asset={withoutEnglish} contentLanguage="en" loading={false} {...handlers} />);
    expect(screen.getByText(/No English version/)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "中文素材" })).toBeInTheDocument();
  });

  it("shows the stored invalid-link reason and allows a recheck", () => {
    render(<DetailPanel asset={{ ...asset, linkCheckStatus: "invalid", linkCheckedAt: "2026-09-18T01:00:00Z", linkCheckMessage: "分享已取消" }} contentLanguage="zh-CN" loading={false} {...handlers} />);
    expect(screen.getByText("网盘链接已失效")).toBeInTheDocument();
    expect(screen.getByText("分享已取消")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /重新检查/ }));
    expect(handlers.onCheckLink).toHaveBeenCalled();
  });

  it("opens only the detected HTTP link from a description", () => {
    const onOpenExternal = vi.fn();
    const linked: AssetDetail = { ...asset, localizations: { ...asset.localizations, "zh-CN": { ...asset.localizations["zh-CN"]!, description: "文档 https://example.com/help。" } } };
    render(<DetailPanel asset={linked} contentLanguage="zh-CN" loading={false} {...handlers} onOpenExternal={onOpenExternal} />);
    fireEvent.click(screen.getByRole("button", { name: "https://example.com/help。" }));
    expect(onOpenExternal).toHaveBeenCalledWith("https://example.com/help");
  });

  it("shows a neutral missing-link state", () => {
    render(<DetailPanel asset={{ ...asset, shareUrl: "" }} contentLanguage="zh-CN" loading={false} {...handlers} />);
    expect(screen.getByText("未添加百度网盘链接")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /重新检查/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "打开百度网盘" })).not.toBeInTheDocument();
  });
});

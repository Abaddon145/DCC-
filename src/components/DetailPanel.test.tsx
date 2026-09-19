import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DetailPanel } from "./DetailPanel";
import type { AssetDetail } from "../types";

vi.mock("./ImagePreview", () => ({ ImagePreview: ({ alt }: { alt: string }) => <img alt={alt} /> }));
vi.mock("./ImageLightbox", () => ({ ImageLightbox: () => null }));

const asset: AssetDetail = {
  id: "1", name: "中文素材", categoryName: null, tags: ["环境"], dccTools: ["Unreal Engine"], versions: ["5.5"], formats: ["uasset"], favorite: false,
  updatedAt: "2026-01-01T00:00:00Z", lastViewedAt: null, coverImageId: null, contentLanguage: "zh-CN", languageFallback: false,
  linkCheckStatus: "unknown", linkCheckedAt: null, linkCheckMessage: "",
  description: "中文描述", categoryId: null, sizeBytes: null, author: "Studio", sourceUrl: "", license: "中文许可", shareUrl: "https://pan.baidu.com/s/demo", extractionCode: "", images: [], createdAt: "2026-01-01T00:00:00Z",
  localizations: {
    "zh-CN": { name: "中文素材", description: "中文描述", tags: ["环境"], license: "中文许可" },
    en: { name: "English Asset", description: "English description", tags: ["Environment"], license: "English license" }
  }
};

const handlers = { onClose: vi.fn(), onEdit: vi.fn(), onDelete: vi.fn(), onFavorite: vi.fn(), onOpen: vi.fn(), onSourceOpen: vi.fn(), onCopy: vi.fn(), onCheckLink: vi.fn(), checkingLink: false };

describe("DetailPanel bilingual content", () => {
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
});

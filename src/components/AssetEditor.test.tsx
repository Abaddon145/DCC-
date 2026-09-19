import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AssetEditor } from "./AssetEditor";
import { api } from "../lib/api";

vi.mock("../lib/api", () => ({ api: {
  translateAssetFields: vi.fn(), getTranslationSettings: vi.fn(), fetchFabMetadata: vi.fn(),
  parseShareText: vi.fn(), readClipboard: vi.fn(), checkShareUrl: vi.fn()
} }));
vi.mock("./ImagePreview", () => ({ ImagePreview: () => <div /> }));

describe("AssetEditor online translation", () => {
  beforeEach(() => vi.clearAllMocks());

  it("previews translated fields and only writes them after confirmation", async () => {
    vi.mocked(api.translateAssetFields).mockResolvedValue({
      fields: { name: "Forest Ruins", description: "Environment pack", tags: ["Environment"], license: "Commercial" },
      characterCount: 12, warnings: [], failedFields: []
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
});

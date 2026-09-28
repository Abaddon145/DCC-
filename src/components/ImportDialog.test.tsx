import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { ImportDialog } from "./ImportDialog";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => undefined) }));
vi.mock("../lib/api", () => ({ api: {
  inspectImport: vi.fn(), commitImport: vi.fn(), getTranslationSettings: vi.fn(), batchTranslateMissingAssets: vi.fn(),
} }));

describe("ImportDialog", () => {
  beforeEach(() => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.mocked(open).mockResolvedValue("C:\\Fab.xlsx");
    vi.mocked(api.getTranslationSettings).mockResolvedValue({ provider: "baidu", configured: false, fabAutoTranslate: true, contentLanguage: "zh-CN" });
    vi.mocked(api.inspectImport).mockResolvedValue({
      headers: ["fab_url"], suggestedMapping: { fab_url: "fab_url" }, sample: [],
      validCount: 1, warningCount: 0, errorCount: 0, duplicateCount: 0,
      rows: [{ row: 2, name: "Fab Asset", status: "valid", messages: [] }],
    });
    vi.mocked(api.commitImport).mockResolvedValue({
      imported: 1, skipped: 0, failed: 0,
      rows: [{ row: 2, name: "Fab Asset", status: "warning", messages: ["自动翻译跳过：尚未配置百度翻译凭据"] }],
    });
  });

  it("shows per-row translation failures after import instead of leaving the preview table", async () => {
    render(<ImportDialog onClose={vi.fn()} onImported={vi.fn()} notify={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "选择文件" }));
    expect(await screen.findByText(/尚未确认翻译凭据/)).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "导入 1 条" }));
    expect(await screen.findByText("逐行导入结果")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("自动翻译跳过：尚未配置百度翻译凭据")).toBeInTheDocument());
    expect(screen.getByText(/导入完成：新增 1/)).toBeInTheDocument();
    expect(window.confirm).toHaveBeenCalled();
  });

  it("requests bilingual completion by default even when the single-Fab setting is off", async () => {
    vi.mocked(api.getTranslationSettings).mockResolvedValue({ provider: "baidu", configured: true, fabAutoTranslate: false, contentLanguage: "zh-CN" });
    render(<ImportDialog onClose={vi.fn()} onImported={vi.fn()} notify={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "选择文件" }));
    expect(await screen.findByRole("checkbox", { name: "自动补齐中英双语" })).toBeChecked();
    fireEvent.click(await screen.findByRole("button", { name: "导入 1 条" }));
    await waitFor(() => expect(api.commitImport).toHaveBeenCalledWith("C:\\Fab.xlsx", { fab_url: "fab_url" }, true));
  });

  it("does not start an import without credentials until the user accepts source-only records", async () => {
    vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<ImportDialog onClose={vi.fn()} onImported={vi.fn()} notify={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "选择文件" }));
    fireEvent.click(await screen.findByRole("button", { name: "导入 1 条" }));
    expect(api.commitImport).not.toHaveBeenCalled();
  });

  it("retries pending translations from the per-row report", async () => {
    vi.mocked(api.getTranslationSettings).mockResolvedValue({ provider: "baidu", configured: true, fabAutoTranslate: false, contentLanguage: "zh-CN" });
    vi.mocked(api.commitImport).mockResolvedValue({ imported: 1, skipped: 0, failed: 0, rows: [{ row: 2, name: "Desert Dune", status: "translationPending", assetId: "asset-1", messages: ["待补译：中文描述"] }] });
    vi.mocked(api.batchTranslateMissingAssets).mockResolvedValue({ completed: 1, pending: 0, skipped: 0, rows: [{ assetId: "asset-1", name: "Desert Dune", status: "complete", messages: [] }] });
    render(<ImportDialog onClose={vi.fn()} onImported={vi.fn()} notify={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "选择文件" }));
    fireEvent.click(await screen.findByRole("button", { name: "导入 1 条" }));
    fireEvent.click(await screen.findByRole("button", { name: "重试待补译 1 条" }));
    await waitFor(() => expect(api.batchTranslateMissingAssets).toHaveBeenCalledWith(["asset-1"]));
    expect(await screen.findByText(/待补译 0/)).toBeInTheDocument();
  });
});

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { ImportDialog } from "./ImportDialog";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => undefined) }));
vi.mock("../lib/api", () => ({ api: {
  inspectImport: vi.fn(), commitImport: vi.fn(), getTranslationSettings: vi.fn(),
} }));

describe("ImportDialog", () => {
  beforeEach(() => {
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
    expect(await screen.findByText(/尚未配置百度翻译凭据/)).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "导入 1 条" }));
    expect(await screen.findByText("逐行导入结果")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("自动翻译跳过：尚未配置百度翻译凭据")).toBeInTheDocument());
    expect(screen.getByText(/导入完成：新增 1/)).toBeInTheDocument();
  });
});

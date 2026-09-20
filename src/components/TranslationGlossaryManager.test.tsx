import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TranslationGlossaryManager } from "./TranslationGlossaryManager";
import { api } from "../lib/api";
import { open, save } from "@tauri-apps/plugin-dialog";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../lib/api", () => ({ api: {
  listTranslationTerms: vi.fn(), upsertTranslationTerm: vi.fn(), deleteTranslationTerm: vi.fn(),
  setTranslationTermEnabled: vi.fn(), resetTranslationTerms: vi.fn(),
  importTranslationTerms: vi.fn(), exportTranslationTerms: vi.fn(),
} }));

const builtin = { id: "builtin-en-zhCN-0001", sourceLanguage: "en" as const, targetLanguage: "zh-CN" as const, source: "Skeletal Mesh", target: "骨骼网格体", mode: "translate" as const, caseSensitive: false, enabled: true, origin: "builtin" as const };

describe("TranslationGlossaryManager", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.listTranslationTerms).mockResolvedValue([builtin]);
    vi.mocked(api.upsertTranslationTerm).mockResolvedValue({ ...builtin, id: "custom-1", origin: "custom" });
  });

  it("searches terms and creates a custom override from a builtin", async () => {
    render(<TranslationGlossaryManager notify={vi.fn()} />);
    expect(await screen.findByText("Skeletal Mesh")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("搜索专业术语"), { target: { value: "Mesh" } });
    await waitFor(() => expect(api.listTranslationTerms).toHaveBeenLastCalledWith(expect.objectContaining({ query: "Mesh" })));
    fireEvent.click(screen.getByTitle("创建自定义覆盖"));
    fireEvent.change(screen.getByDisplayValue("骨骼网格体"), { target: { value: "骨架网格体" } });
    fireEvent.click(screen.getByRole("button", { name: "保存术语" }));
    await waitFor(() => expect(api.upsertTranslationTerm).toHaveBeenCalledWith(expect.objectContaining({ source: "Skeletal Mesh", target: "骨架网格体", id: null })));
  });

  it("imports and exports the global glossary as Excel", async () => {
    vi.mocked(open).mockResolvedValue("C:\\terms.xlsx" as never);
    vi.mocked(save).mockResolvedValue("C:\\export.xlsx" as never);
    vi.mocked(api.importTranslationTerms).mockResolvedValue({ imported: 2, updated: 1, skipped: 0, warnings: [] });
    vi.mocked(api.exportTranslationTerms).mockResolvedValue(undefined);
    render(<TranslationGlossaryManager notify={vi.fn()} />);
    await screen.findByText("Skeletal Mesh");
    fireEvent.click(screen.getByRole("button", { name: "导入 Excel" }));
    await waitFor(() => expect(api.importTranslationTerms).toHaveBeenCalledWith("C:\\terms.xlsx"));
    fireEvent.click(screen.getByRole("button", { name: "导出 Excel" }));
    await waitFor(() => expect(api.exportTranslationTerms).toHaveBeenCalledWith("C:\\export.xlsx"));
  });
});

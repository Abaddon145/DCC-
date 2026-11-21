import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import App from "./App";
import { api } from "./lib/api";
import { defaultGlobalPreferences, defaultLibraryPreferences } from "./lib/personalization";
import type { BatchUpdateReport } from "./types";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("./lib/api", async importOriginal => {
  const original = await importOriginal<typeof import("./lib/api")>();
  return { ...original, api: { ...original.api,
    getWorkspaceContext: vi.fn().mockResolvedValue({ libraryId: "lib", enabled: false, value: null }),
    saveWorkspaceContext: vi.fn().mockResolvedValue(true), saveLibraryPreferences: vi.fn().mockResolvedValue(true),
    getPersonalization: vi.fn(), getTranslationSettings: vi.fn().mockResolvedValue({ contentLanguage: "zh-CN" }),
    getMeta: vi.fn().mockResolvedValue({ categories: [], filters: { tags: [], dccTools: [], versions: [], formats: [], licenses: [] }, totalAssets: 2 }),
    listSmartCollections: vi.fn().mockResolvedValue([]), search: vi.fn().mockResolvedValue({ items: [], total: 2, offset: 0, limit: 80 }),
    batchUpdate: vi.fn(),
  } };
});
vi.mock("./components/TaskCenter", () => ({ TaskCenter: () => null }));
vi.mock("./components/ReliabilitySettings", () => ({ AutomaticBackupService: () => null }));
vi.mock("./components/AssetGrid", () => ({ AssetGrid: ({ onToggleSelect, selectedIds }: { onToggleSelect?: (id: string) => void; selectedIds?: Set<string> }) => <div><button onClick={() => onToggleSelect?.("asset-a")}>选择甲</button><button onClick={() => onToggleSelect?.("asset-b")}>选择乙</button><output aria-label="当前选中">{[...(selectedIds ?? [])].sort().join(",")}</output></div> }));
vi.mock("./features/assets/AssetBrowser", () => ({ AssetBrowser: () => null, AssetScope: ({ children }: { children: React.ReactNode }) => <>{children}</> }));

it("keeps a selection made while a batch request is pending", async () => {
  let finish: (report: BatchUpdateReport) => void = () => {};
  vi.mocked(api.batchUpdate).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  vi.mocked(api.getPersonalization).mockResolvedValue({ global: defaultGlobalPreferences, library: defaultLibraryPreferences });
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "多选" }));
  fireEvent.click(screen.getByRole("button", { name: "选择甲" }));
  fireEvent.change(screen.getByLabelText("批量收藏"), { target: { value: "true" } });
  fireEvent.click(screen.getByRole("button", { name: "应用" }));
  await waitFor(() => expect(api.batchUpdate).toHaveBeenCalledWith(expect.objectContaining({ ids: ["asset-a"], favorite: true })));
  fireEvent.click(screen.getByRole("button", { name: "清空" }));
  fireEvent.click(screen.getByRole("button", { name: "选择乙" }));
  expect(screen.getByLabelText("当前选中")).toHaveTextContent("asset-b");
  await act(async () => { finish({ requested: 1, updated: 1, identical: false }); });
  expect(screen.getByLabelText("当前选中")).toHaveTextContent("asset-b");
  expect(screen.getByLabelText("当前选中")).not.toHaveTextContent("asset-a");
});

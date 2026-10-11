import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import App from "./App";
import { api } from "./lib/api";
import { defaultGlobalPreferences, defaultLibraryPreferences } from "./lib/personalization";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("./lib/api", async importOriginal => {
  const original = await importOriginal<typeof import("./lib/api")>();
  return { ...original, api: { ...original.api,
    getWorkspaceContext: vi.fn().mockResolvedValue({ libraryId: "lib", enabled: false, value: null }),
    saveWorkspaceContext: vi.fn().mockResolvedValue(true), saveLibraryPreferences: vi.fn().mockResolvedValue(true),
    getPersonalization: vi.fn(), getTranslationSettings: vi.fn().mockResolvedValue({ contentLanguage: "zh-CN" }),
    getMeta: vi.fn().mockResolvedValue({ categories: [], filters: { tags: [], dccTools: [], versions: [], formats: [], licenses: [] }, totalAssets: 0 }),
    listSmartCollections: vi.fn().mockResolvedValue([]), search: vi.fn().mockResolvedValue({ items: [], total: 0, offset: 0, limit: 80 }),
    selectAssetIds: vi.fn().mockResolvedValue({ ids: ["hidden"], total: 1 }),
    getDeleteImpact: vi.fn().mockResolvedValue({ categoryCount: 1, assetCount: 1 }), deleteLibraryItems: vi.fn(),
  } };
});
vi.mock("./components/TaskCenter", () => ({ TaskCenter: () => null }));
vi.mock("./components/AssetGrid", () => ({ AssetGrid: () => null }));
vi.mock("./components/ReliabilitySettings", () => ({ AutomaticBackupService: () => null }));
vi.mock("./components/CategoryTree", () => ({ CategoryTree: ({ onChange }: { onChange: (ids: string[]) => void }) => <button onClick={() => onChange(["hidden-category"])}>测试分类</button> }));
vi.mock("./features/assets/AssetBrowser", async importOriginal => {
  const original = await importOriginal<typeof import("./features/assets/AssetBrowser")>();
  return { ...original, AssetBrowser: () => <section>统一浏览测试</section> };
});
afterEach(() => { cleanup(); vi.clearAllMocks(); });
it("全部资产隔离隐藏分类、选择与删除快捷键，切回普通范围后恢复", async () => {
  vi.mocked(api.getPersonalization).mockResolvedValue({ global: defaultGlobalPreferences, library: defaultLibraryPreferences });
  vi.spyOn(window, "confirm").mockReturnValue(false);
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "测试分类" }));
  await waitFor(() => expect(api.search).toHaveBeenCalledWith(expect.objectContaining({ categoryIds: ["hidden-category"] })));
  fireEvent.click(screen.getByRole("button", { name: "全部资产" }));
  await screen.findByText("统一浏览测试");
  fireEvent.keyDown(document.body, { key: "Delete" });
  fireEvent.keyDown(document.body, { key: "m", ctrlKey: true });
  fireEvent.keyDown(document.body, { key: "a", ctrlKey: true });
  await new Promise(resolve => setTimeout(resolve, 0));
  expect(api.selectAssetIds).not.toHaveBeenCalled();
  expect(api.getDeleteImpact).not.toHaveBeenCalled();
  expect(api.deleteLibraryItems).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "普通素材" }));
  const dialog = document.createElement("aside"); dialog.className = "core-detail"; document.body.append(dialog);
  fireEvent.keyDown(document.body, { key: "Delete" });
  expect(api.getDeleteImpact).not.toHaveBeenCalled(); dialog.remove();
  fireEvent.keyDown(document.body, { key: "Delete" });
  await waitFor(() => expect(api.getDeleteImpact).toHaveBeenCalledWith({ assetIds: [], categoryId: "hidden-category" }));
});

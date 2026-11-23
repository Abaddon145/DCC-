import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import { FabLibraryDialog } from "./FabLibraryDialog";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => undefined) }));
vi.mock("../lib/api", () => ({
  api: {
    getTranslationSettings: vi.fn(),
    fabAccountStatus: vi.fn(),
    beginFabLogin: vi.fn(),
    logoutFabAccount: vi.fn(),
    previewOwnedFabLibrary: vi.fn(),
    stageOwnedFabImport: vi.fn(),
    commitImport: vi.fn(),
  },
}));

describe("FabLibraryDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.getTranslationSettings).mockResolvedValue({
      provider: "baidu",
      configured: false,
      fabAutoTranslate: false,
      contentLanguage: "zh-CN",
    });
    vi.mocked(api.fabAccountStatus).mockResolvedValue({
      signedIn: true,
      displayName: "Tester",
      message: "",
    });
    vi.mocked(api.previewOwnedFabLibrary).mockResolvedValue({
      displayName: "Tester",
      fetched: 2,
      newCount: 2,
      duplicateCount: 0,
      skippedCount: 0,
      rows: [
        { listingId: "a", title: "Norway Maple", url: "https://www.fab.com/listings/a", status: "new", message: "" },
        { listingId: "b", title: "European Beech", url: "https://www.fab.com/listings/b", status: "new", message: "" },
      ],
    });
  });

  it("uses a scrollable import-body container so the footer import button stays visible", async () => {
    const { container } = render(<FabLibraryDialog onClose={vi.fn()} onImported={vi.fn()} notify={vi.fn()} />);
    expect(await screen.findByText("已登录 Tester")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "读取已购库" }));
    expect(await screen.findByText("Norway Maple")).toBeInTheDocument();
    expect(container.querySelector(".import-modal > .import-body.fab-library-body")).not.toBeNull();
    expect(container.querySelector(".import-modal > .modal-footer")).not.toBeNull();
    expect(screen.getByRole("button", { name: "导入 2 条新商品" })).toBeEnabled();
  });
});

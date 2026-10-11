import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import type { MediaEntryDetail } from "../types";
import { MediaDetail } from "./MediaLibraryView";

vi.mock("../lib/api", () => ({
  mediaLibraryFileUrl: (id: string) => `http://dcc-media.localhost/library/file/${id}`,
  mediaLibraryBundleUrl: (id: string, path: string) => `http://dcc-media.localhost/library/bundle/${id}/${path}`,
  api: { listProjects: vi.fn(), listReferenceBoards: vi.fn(), updateMediaEntry: vi.fn(), linkProjectMedia: vi.fn(), addMediaImagesToBoard: vi.fn(), openExternal: vi.fn() },
}));
vi.mock("./ModelViewer", () => ({ ModelViewer: () => null }));
const libraryId = "11111111-1111-4111-8111-111111111111";
const detail: MediaEntryDetail = {
  id: "image", kind: "image", folderId: null, name: "参考图", description: "", author: "", sourceUrl: "https://example.com",
  license: "", favorite: false, processingStatus: "ready", processingMessage: "", format: "png", fileSize: 1024,
  tags: [], thumbnailFileId: null, primaryFileId: "main", createdAt: "2026-10-02", updatedAt: "2026-10-02", assetIds: [], projectIds: [],
  files: [{ id: "main", entryId: "image", role: "main", logicalPath: "image.png", originalName: "image.png", mimeType: "image/png", fileSize: 1024, checksum: "", width: 100, height: 100, durationMs: null }],
};

describe("captured media detail bindings", () => {
  it("shows media dimensions and provides its saved source link", async () => {render(<MediaDetail detail={detail} canAddProject={false} canAddBoard={false} onClose={vi.fn()} onChanged={async()=>{}} onNotify={vi.fn()}/>);expect(screen.getByText("100 × 100")).toBeInTheDocument();fireEvent.click(screen.getByRole("button",{name:"打开来源网页"}));await waitFor(()=>expect(api.openExternal).toHaveBeenCalledWith(detail.sourceUrl));});
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.openExternal).mockResolvedValue();
    vi.mocked(api.listProjects).mockResolvedValue([{ id: "project", name: "作品" }] as Awaited<ReturnType<typeof api.listProjects>>);
    vi.mocked(api.listReferenceBoards).mockResolvedValue([{ id: "board", name: "灵感板" }] as Awaited<ReturnType<typeof api.listReferenceBoards>>);
    vi.mocked(api.updateMediaEntry).mockResolvedValue();
    vi.mocked(api.linkProjectMedia).mockResolvedValue(1);
    vi.mocked(api.addMediaImagesToBoard).mockResolvedValue({ items: [], skipped: 1, warnings: ["原图不可用"] });
  });

  it("keeps editing and project/reference mutations bound to the capture library", async () => {
    const notify = vi.fn();
    render(<MediaDetail detail={detail} expectedLibraryId={libraryId} onClose={vi.fn()} onChanged={async () => {}} onNotify={notify} />);
    await screen.findByRole("option", { name: "作品" });
    const selects = screen.getAllByRole("combobox");
    fireEvent.change(selects[0], { target: { value: "project" } });
    fireEvent.click(screen.getByRole("button", { name: "加入项目" }));
    await waitFor(() => expect(api.linkProjectMedia).toHaveBeenCalledWith(["image"], ["project"], libraryId));
    fireEvent.change(selects[1], { target: { value: "board" } });
    fireEvent.click(screen.getByRole("button", { name: "复制到参考板" }));
    await waitFor(() => expect(api.addMediaImagesToBoard).toHaveBeenCalledWith("board", ["image"], { x: 0, y: 0 }, libraryId));
    fireEvent.click(screen.getByRole("button", { name: "编辑基本信息" }));
    fireEvent.change(screen.getByRole("textbox", { name: "名称" }), { target: { value: "新的参考图" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(api.updateMediaEntry).toHaveBeenCalledWith("image", expect.objectContaining({ name: "新的参考图" }), libraryId));
  });

  it("does not load or render disabled association modules", () => {
    render(<MediaDetail detail={detail} canAddProject={false} canAddBoard={false} onClose={vi.fn()} onChanged={async () => {}} onNotify={vi.fn()} />);
    expect(api.listProjects).not.toHaveBeenCalled();
    expect(api.listReferenceBoards).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "加入项目" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "复制到参考板" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "编辑基本信息" })).toBeInTheDocument();
  });
});

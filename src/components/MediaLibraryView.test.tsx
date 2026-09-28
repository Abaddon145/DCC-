import { useEffect } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import { MediaLibraryView } from "./MediaLibraryView";

vi.mock("../lib/api", () => ({ mediaLibraryBundleUrl: (id: string, path: string) => `http://dcc-media.localhost/library/bundle/${id}/${path}`, mediaLibraryFileUrl: (id: string) => `http://dcc-media.localhost/library/file/${id}`, api: {
  searchMediaEntries: vi.fn(), listMediaFolders: vi.fn(), saveMediaFolder: vi.fn(), getMediaEntry: vi.fn(), queueModelThumbnailTasks: vi.fn().mockResolvedValue(1), saveModelThumbnail: vi.fn(), pendingModelThumbnails: vi.fn(), reportModelThumbnailFailure: vi.fn(),
} }));
vi.mock("./ModelViewer", () => ({ ModelViewer: ({ onReady }: { onReady?: (capture: () => Promise<Uint8Array>) => void }) => {
  useEffect(() => { onReady?.(async () => new Uint8Array([1, 2, 3])); }, []);
  return <div data-testid="thumbnail-model-renderer" />;
} }));
vi.mock("@tanstack/react-virtual", () => ({ useVirtualizer: ({ count }: { count: number }) => ({ getTotalSize: () => count * 236, getVirtualItems: () => count ? [{ key: 0, index: 0, start: 0 }] : [] }) }));

describe("MediaLibraryView folder creation", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
    vi.mocked(api.searchMediaEntries).mockResolvedValue({ items: [], total: 0, offset: 0, limit: 160 });
    vi.mocked(api.listMediaFolders).mockResolvedValue([]);
    vi.mocked(api.saveMediaFolder).mockResolvedValue({ id: "folder-1", kind: "image", parentId: null, name: "概念图", sortOrder: 0, entryCount: 0 });
    vi.mocked(api.pendingModelThumbnails).mockResolvedValue([]);
  });

  it("opens a folder name dialog without collapsing the pane, then saves", async () => {
    const onPaneCollapsedChange = vi.fn();
    render(<MediaLibraryView kind="image" onNotify={vi.fn()} paneWidth={220} paneCollapsed={false} onPaneWidthChange={vi.fn()} onPaneCollapsedChange={onPaneCollapsedChange} />);
    fireEvent.click(await screen.findByRole("button", { name: "新建根文件夹" }));
    expect(onPaneCollapsedChange).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole("textbox", { name: "文件夹名称" }), { target: { value: "概念图" } });
    fireEvent.click(screen.getByRole("button", { name: "创建文件夹" }));
    await waitFor(() => expect(api.saveMediaFolder).toHaveBeenCalledWith("image", null, null, "概念图"));
  });

  it("uses the same root-row and three-dot child creation navigation as the asset tree", async () => {
    const folder = { id: "parent", kind: "image" as const, parentId: null, name: "概念", sortOrder: 0, entryCount: 0 };
    const child = { ...folder, id: "folder-1", parentId: folder.id, name: "角色" };
    vi.mocked(api.listMediaFolders).mockImplementation(async () => vi.mocked(api.saveMediaFolder).mock.calls.length ? [folder, child] : [folder]);
    vi.mocked(api.saveMediaFolder).mockResolvedValue(child);
    const { container } = render(<MediaLibraryView kind="image" onNotify={vi.fn()} paneWidth={220} paneCollapsed={false} onPaneWidthChange={vi.fn()} onPaneCollapsedChange={vi.fn()} />);
    const root = await screen.findByRole("button", { name: "新建根文件夹" });
    expect(root.closest(".category-row.root")).toBeInTheDocument();
    const row = await screen.findByRole("button", { name: /^概念0$/ });
    fireEvent.click(row.closest(".category-row")!.querySelector(".tree-toggle")!);
    const menu = row.closest(".category-row")!.querySelector(".row-menu summary")!;
    fireEvent.click(menu);
    fireEvent.click(screen.getByRole("button", { name: "新建子文件夹" }));
    fireEvent.change(screen.getByRole("textbox", { name: "文件夹名称" }), { target: { value: "角色" } });
    fireEvent.click(screen.getByRole("button", { name: "创建文件夹" }));
    await waitFor(() => expect(api.saveMediaFolder).toHaveBeenCalledWith("image", null, "parent", "角色"));
    expect(await screen.findByRole("button", { name: /^角色0$/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^角色0$/ }).closest(".category-row")).toHaveClass("selected");
    expect(container.querySelector(".folder-action")).toBeNull();
  });

  it("closes a folder action menu after leaving its full range", async () => {
    vi.mocked(api.listMediaFolders).mockResolvedValue([{ id: "parent", kind: "image", parentId: null, name: "概念", sortOrder: 0, entryCount: 0 }]);
    const { container } = render(<MediaLibraryView kind="image" onNotify={vi.fn()} paneWidth={220} paneCollapsed={false} onPaneWidthChange={vi.fn()} onPaneCollapsedChange={vi.fn()} />);
    await screen.findByRole("button", { name: /^概念0$/ });
    const details = container.querySelector<HTMLDetailsElement>(".media-folder-tree .row-menu")!;
    fireEvent.click(details.querySelector("summary")!);
    expect(details.open).toBe(true);
    vi.useFakeTimers();
    fireEvent.mouseLeave(details);
    act(() => vi.advanceTimersByTime(200));
    expect(details.open).toBe(false);
    vi.useRealTimers();
  });

  it("clicking the selected folder again returns to all media", async () => {
    vi.mocked(api.listMediaFolders).mockResolvedValue([{ id: "parent", kind: "image", parentId: null, name: "概念", sortOrder: 0, entryCount: 0 }]);
    render(<MediaLibraryView kind="image" onNotify={vi.fn()} paneWidth={220} paneCollapsed={false} onPaneWidthChange={vi.fn()} onPaneCollapsedChange={vi.fn()} />);
    const row = await screen.findByRole("button", { name: /^概念0$/ });
    fireEvent.click(row);
    await waitFor(() => expect(api.searchMediaEntries).toHaveBeenCalledWith(expect.objectContaining({ folderId: "parent" })));
    fireEvent.click(row);
    await waitFor(() => expect(api.searchMediaEntries).toHaveBeenLastCalledWith(expect.objectContaining({ folderId: null })));
  });

  it("does not show an older media kind's folders when its request finishes late", async () => {
    let finishImageFolders!: (folders: Awaited<ReturnType<typeof api.listMediaFolders>>) => void;
    vi.mocked(api.listMediaFolders).mockImplementation(kind => kind === "image"
      ? new Promise(resolve => { finishImageFolders = resolve; })
      : Promise.resolve([{ id: "model-folder", kind: "model", parentId: null, name: "模型目录", sortOrder: 0, entryCount: 0 }]));
    const props = { onNotify: vi.fn(), paneWidth: 220, paneCollapsed: false, onPaneWidthChange: vi.fn(), onPaneCollapsedChange: vi.fn() };
    const { rerender } = render(<MediaLibraryView kind="image" {...props} />);
    await waitFor(() => expect(api.listMediaFolders).toHaveBeenCalledWith("image"));
    rerender(<MediaLibraryView kind="model" {...props} />);
    expect(await screen.findByRole("button", { name: /^模型目录0$/ })).toBeInTheDocument();
    await act(async () => { finishImageFolders([{ id: "image-folder", kind: "image", parentId: null, name: "图片目录", sortOrder: 0, entryCount: 0 }]); });
    expect(screen.queryByRole("button", { name: /^图片目录0$/ })).not.toBeInTheDocument();
  });

  it("backfills a model thumbnail and replaces the 3D placeholder", async () => {
    const model = { id: "model-1", kind: "model" as const, folderId: null, name: "沙漠沙丘", description: "", author: "", sourceUrl: "", license: "", favorite: false, processingStatus: "ready", processingMessage: "", format: "glb", fileSize: 1024, tags: [], thumbnailFileId: null, primaryFileId: "main-1", createdAt: "2026-01-01", updatedAt: "2026-01-01" };
    vi.mocked(api.searchMediaEntries).mockResolvedValue({ items: [model], total: 1, offset: 0, limit: 160 });
    vi.mocked(api.getMediaEntry).mockResolvedValue({ ...model, files: [{ id: "main-1", entryId: model.id, role: "main", logicalPath: "dune.glb", originalName: "dune.glb", mimeType: "model/gltf-binary", fileSize: 1024, checksum: "", width: null, height: null, durationMs: null }], assetIds: [], projectIds: [] });
    vi.mocked(api.pendingModelThumbnails).mockResolvedValue([{ id: model.id, logicalPath: "dune.glb", originalName: "dune.glb" }]);
    vi.mocked(api.saveModelThumbnail).mockResolvedValue("thumb-1");
    render(<MediaLibraryView kind="model" onNotify={vi.fn()} paneWidth={220} paneCollapsed={false} onPaneWidthChange={vi.fn()} onPaneCollapsedChange={vi.fn()} />);
    await waitFor(() => expect(api.queueModelThumbnailTasks).toHaveBeenCalled());
    expect(api.saveModelThumbnail).not.toHaveBeenCalled();
    vi.mocked(api.searchMediaEntries).mockResolvedValue({items:[{...model,thumbnailFileId:"thumb-1"}],total:1,offset:0,limit:160});
    act(()=>window.dispatchEvent(new CustomEvent("library-task-completed")));
    expect(await screen.findByRole("img", { name: "沙漠沙丘" })).toHaveAttribute("src", expect.stringContaining("thumb-1"));
  });
});

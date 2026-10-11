import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import type { BrowserCapture, MediaEntryDetail } from "../types";
import { BrowserCaptureInbox } from "./BrowserCaptureInbox";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../lib/api", () => ({
  mediaLibraryFileUrl: (id: string) => `http://dcc-media.localhost/library/file/${id}`,
  api: {
    getWorkspaceContext: vi.fn(), listBrowserCaptures: vi.fn(), registerBrowserCaptureHost: vi.fn(),
    organizeBrowserCapture: vi.fn(), createBrowserCaptureNote: vi.fn(), getMediaEntry: vi.fn(),
    listMediaFolders: vi.fn(), batchUpdateMediaEntries: vi.fn(), openExternal: vi.fn(),
  },
}));
vi.mock("./MediaLibraryView", () => ({
  MediaDetail: ({ detail, onClose }: { detail: MediaEntryDetail; onClose: () => void }) =>
    <aside className="media-detail-panel" aria-label="媒体预览"><span>{detail.name}</span><button onClick={onClose}>关闭预览</button></aside>,
}));
vi.mock("./MediaAssociationPicker", () => ({ MediaAssociationPicker: () => null }));

const libraryId = "11111111-1111-4111-8111-111111111111";
function capture(overrides: Partial<BrowserCapture> = {}): BrowserCapture {
  return {
    id: "capture", title: "场景参考", pageUrl: "https://example.com/reference", kind: "image",
    entryId: "image", status: "saved", message: "图片文件已保存", createdAt: "2026-10-02T12:00:00Z",
    organized: false, thumbnailId: "thumbnail", unavailable: false, ...overrides,
  };
}
function article(title: string) { return screen.getByRole("heading", { name: title }).closest("article")!; }

describe("BrowserCaptureInbox", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.getWorkspaceContext).mockResolvedValue({ libraryId, enabled: true, value: null });
    vi.mocked(api.listBrowserCaptures).mockResolvedValue({ items: [capture()], total: 1, offset: 0, limit: 100 });
    vi.mocked(api.registerBrowserCaptureHost).mockResolvedValue("浏览器已连接");
    vi.mocked(api.organizeBrowserCapture).mockResolvedValue();
    vi.mocked(api.createBrowserCaptureNote).mockResolvedValue("new-note");
    vi.mocked(api.listMediaFolders).mockResolvedValue([{ id: "folder", kind: "image", parentId: null, name: "环境", sortOrder: 0, entryCount: 0 }]);
    vi.mocked(api.batchUpdateMediaEntries).mockResolvedValue({ requested: 1, updated: 1 });
    vi.mocked(api.openExternal).mockResolvedValue();
    vi.mocked(api.getMediaEntry).mockResolvedValue({ id: "image", name: "已保存图片" } as MediaEntryDetail);
  });

  it("distinguishes downloaded files, source-only links, duplicates and unavailable media", async () => {
    vi.mocked(api.listBrowserCaptures).mockResolvedValue({ total: 4, offset: 0, limit: 100, items: [capture(),
      capture({ id: "link", title: "平台视频", kind: "link", entryId: null, status: "link", thumbnailId: null, message: "未下载视频文件" }),
      capture({ id: "duplicate", title: "重复参考", status: "duplicate", message: "已有相同文件" }),
      capture({ id: "trash", title: "已删除参考", unavailable: true }),
    ] });
    render(<BrowserCaptureInbox notify={vi.fn()} />);
    await screen.findByRole("heading", { name: "平台视频" });
    expect(within(article("场景参考")).getByText("文件已保存 · 待整理")).toBeInTheDocument();
    expect(within(article("平台视频")).getByText("仅来源链接 · 待整理")).toBeInTheDocument();
    expect(within(article("平台视频")).queryByRole("button", { name: "打开预览" })).not.toBeInTheDocument();
    expect(within(article("平台视频")).getByRole("button", { name: "预览 平台视频" })).toBeDisabled();
    expect(within(article("重复参考")).getByRole("button", { name: "打开已有条目" })).toBeEnabled();
    expect(within(article("已删除参考")).getByText(/媒体在回收站/)).toBeInTheDocument();
    expect(within(article("已删除参考")).getByRole("button", { name: "预览 已删除参考" })).toBeDisabled();
    expect(api.listBrowserCaptures).toHaveBeenCalledWith(libraryId, 0);
  });

  it("registers only a valid Chromium extension ID and reports connection result", async () => {
    const notify = vi.fn();
    render(<BrowserCaptureInbox notify={notify} />);
    await screen.findByRole("heading", { name: "场景参考" });
    const input = screen.getByRole("textbox", { name: "扩展 ID" });
    const connect = screen.getByRole("button", { name: "连接浏览器" });
    expect(connect).toBeDisabled();
    fireEvent.change(input, { target: { value: "z".repeat(32) } });
    expect(connect).toBeDisabled();
    fireEvent.change(input, { target: { value: "a".repeat(31) } });
    expect(connect).toBeDisabled();
    fireEvent.change(input, { target: { value: "abcdefghijklmnopabcdefghijklmnop" } });
    fireEvent.click(connect);
    await waitFor(() => expect(api.registerBrowserCaptureHost).toHaveBeenCalledWith("abcdefghijklmnopabcdefghijklmnop"));
    await waitFor(() => expect(notify).toHaveBeenCalledWith("浏览器已连接"));
  });

  it("organizes the saved entry without copying files and binds completion to the loaded library UUID", async () => {
    const notify = vi.fn();
    render(<BrowserCaptureInbox notify={notify} />);
    await screen.findByRole("heading", { name: "场景参考" });
    fireEvent.click(screen.getByRole("button", { name: "文件夹与标签" }));
    const dialog = await screen.findByRole("dialog", { name: "整理采集" });
    fireEvent.change(within(dialog).getByRole("combobox", { name: "文件夹" }), { target: { value: "folder" } });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "添加标签" }), { target: { value: " UE5；环境 ; " } });
    fireEvent.click(within(dialog).getByRole("button", { name: "保存整理" }));
    await waitFor(() => expect(api.batchUpdateMediaEntries).toHaveBeenCalledWith({ ids: ["image"], folderId: "folder", clearFolder: false, addTags: ["UE5", "环境"], removeTags: [], favorite: null }, libraryId));
    await waitFor(() => expect(api.organizeBrowserCapture).toHaveBeenCalledWith(libraryId, "capture"));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(notify).toHaveBeenCalledWith("已整理；媒体原件仍保存在当前素材库");
  });

  it("opens the registered media preview and dismisses it when clicking outside", async () => {
    render(<BrowserCaptureInbox notify={vi.fn()} />);
    await screen.findByRole("heading", { name: "场景参考" });
    fireEvent.click(screen.getByRole("button", { name: "预览 场景参考" }));
    const preview = await screen.findByRole("complementary", { name: "媒体预览" });
    expect(api.getMediaEntry).toHaveBeenCalledWith("image");
    fireEvent.pointerDown(preview);
    expect(preview).toBeInTheDocument();
    fireEvent.pointerDown(screen.getByRole("heading", { name: "采集收件箱" }));
    expect(screen.queryByRole("complementary", { name: "媒体预览" })).not.toBeInTheDocument();
  });

  it("creates a learning note in the loaded library and navigates only after it succeeds", async () => {
    const openNote = vi.fn(); const notify = vi.fn();
    render(<BrowserCaptureInbox notify={notify} onOpenNote={openNote} />);
    await screen.findByRole("heading", { name: "场景参考" });
    fireEvent.click(screen.getByRole("button", { name: "建立学习笔记" }));
    await waitFor(() => expect(api.createBrowserCaptureNote).toHaveBeenCalledWith(libraryId, "capture"));
    await waitFor(() => expect(openNote).toHaveBeenCalledWith("new-note"));
    vi.mocked(api.createBrowserCaptureNote).mockRejectedValueOnce(new Error("素材库已切换"));
    fireEvent.click(screen.getByRole("button", { name: "建立学习笔记" }));
    await waitFor(() => expect(notify).toHaveBeenCalledWith("Error: 素材库已切换", true));
    expect(openNote).toHaveBeenCalledTimes(1);
  });

  it("preserves loaded cards after a failed refresh and exposes an explicit retry", async () => {
    const notify = vi.fn();
    render(<BrowserCaptureInbox notify={notify} />);
    await screen.findByRole("heading", { name: "场景参考" });
    vi.mocked(api.listBrowserCaptures).mockRejectedValueOnce(new Error("读取失败"));
    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("heading", { name: "场景参考" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "把灵感留在这里" })).not.toBeInTheDocument();
    expect(notify).toHaveBeenCalledWith("Error: 读取失败", true);
    fireEvent.click(screen.getByRole("button", { name: "重新读取" }));
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });

  it("does not offer disabled project, reference or knowledge commands", async () => {
    render(<BrowserCaptureInbox notify={vi.fn()} canAddProject={false} canAddBoard={false} />);
    await screen.findByRole("heading", { name: "场景参考" });
    expect(screen.queryByRole("button", { name: "加入项目" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "加入参考板" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "建立学习笔记" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开预览" })).toBeEnabled();
  });
});

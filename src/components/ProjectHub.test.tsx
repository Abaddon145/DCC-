import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CreativeProject, MediaEntry, ProjectSummary, ProjectUnit } from "../types";
import { api } from "../lib/api";
import { ProjectHub } from "./ProjectHub";

vi.mock("../lib/api", () => ({ mediaLibraryFileUrl: (id: string) => `http://dcc-media.localhost/library/file/${id}`, api: {
  listProjects: vi.fn(), getProject: vi.fn(), moveProjectTask: vi.fn(), listReferenceBoards: vi.fn(),
  searchMediaEntries: vi.fn(), saveProject: vi.fn(), setProjectShotVideo: vi.fn(),
} }));

const summary: ProjectSummary = {
  id: "project-1", name: "遗迹短片", description: "UE 镜头练习", projectType: "animation", status: "active",
  targetTools: ["Unreal Engine"], versions: ["5.5"], resolutionWidth: 1920, resolutionHeight: 1080, frameRate: 24,
  coverAssetId: null, coverImageId: null, coverMediaEntryId: null, coverMediaFileId: null, assetCount: 0, unavailableAssetCount: 0, taskCount: 1,
  completedTaskCount: 0, reviewTaskCount: 0, progress: 0, mainBoardId: null,
  createdAt: "2026-01-01", updatedAt: "2026-01-02", lastOpenedAt: null, archivedAt: null,
};
const detail: CreativeProject = {
  ...summary, units: [], assets: [], media: [], boards: [], paths: [], tasks: [{
    id: "task-1", projectId: summary.id, unitId: null, title: "完成灯光", description: "", status: "todo",
    priority: "high", dueDate: null, sortOrder: 0, assetIds: [], createdAt: "2026-01-01", updatedAt: "2026-01-01",
  }],
};
const image: MediaEntry = {
  id: "image-1", kind: "image", folderId: null, name: "概念封面", description: "", author: "", sourceUrl: "", license: "",
  favorite: false, processingStatus: "ready", processingMessage: "", format: "png", fileSize: 512, tags: [],
  thumbnailFileId: "thumb-1", primaryFileId: "file-1", createdAt: "2026-01-01", updatedAt: "2026-01-01",
};

describe("ProjectHub", () => {
  beforeEach(() => {
    vi.mocked(api.listProjects).mockResolvedValue([summary]);
    vi.mocked(api.getProject).mockResolvedValue(detail);
    vi.mocked(api.moveProjectTask).mockResolvedValue();
    vi.mocked(api.listReferenceBoards).mockResolvedValue([]);
    vi.mocked(api.searchMediaEntries).mockResolvedValue({ items: [image], total: 1, offset: 0, limit: 100 });
    vi.mocked(api.saveProject).mockResolvedValue(detail);
    vi.mocked(api.setProjectShotVideo).mockResolvedValue();
  });

  it("opens a project and moves a task across the fixed kanban columns", async () => {
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: /任务 1/ }));
    const task = await screen.findByText("完成灯光");
    fireEvent.dragStart(task.closest("article")!);
    const doneColumn = screen.getByText("完成", { selector: "strong" }).closest("section")!;
    fireEvent.drop(doneColumn);
    await waitFor(() => expect(api.moveProjectTask).toHaveBeenCalledWith("task-1", "done", 0));
  });

  it("allows an existing image-library entry to become the project cover", async () => {
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: "项目设置" }));
    fireEvent.click(await screen.findByRole("button", { name: "概念封面" }));
    fireEvent.click(screen.getByRole("button", { name: "保存项目" }));
    await waitFor(() => expect(api.saveProject).toHaveBeenCalledWith(expect.objectContaining({ coverMediaEntryId: "image-1", coverAssetId: null })));
  });

  it("assigns a replaceable video-library entry to a shot", async () => {
    const scene: ProjectUnit = { id: "scene-1", projectId: summary.id, parentId: null, kind: "scene", name: "场景一", description: "", startFrame: null, endFrame: null, resolutionWidth: null, resolutionHeight: null, frameRate: null, sortOrder: 0, videoMediaEntryId: null, videoStreamFileId: null, videoName: null, createdAt: "now", updatedAt: "now" };
    const shot: ProjectUnit = { ...scene, id: "shot-1", parentId: scene.id, kind: "shot", name: "SH010", startFrame: 1, endFrame: 100 };
    vi.mocked(api.getProject).mockResolvedValue({ ...detail, units: [scene, shot] });
    vi.mocked(api.searchMediaEntries).mockResolvedValue({ items: [{ ...image, id: "video-1", kind: "video", name: "预演视频", format: "mp4" }], total: 1, offset: 0, limit: 100 });
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: "场景 / 镜头" }));
    fireEvent.click(screen.getByRole("button", { name: "从视频库选择" }));
    fireEvent.click(await screen.findByRole("button", { name: /预演视频/ }));
    await waitFor(() => expect(api.setProjectShotVideo).toHaveBeenCalledWith("shot-1", "video-1"));
  });
});

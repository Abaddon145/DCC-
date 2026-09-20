import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CreativeProject, ProjectSummary } from "../types";
import { api } from "../lib/api";
import { ProjectHub } from "./ProjectHub";

vi.mock("../lib/api", () => ({ api: {
  listProjects: vi.fn(), getProject: vi.fn(), moveProjectTask: vi.fn(), listReferenceBoards: vi.fn(),
} }));

const summary: ProjectSummary = {
  id: "project-1", name: "遗迹短片", description: "UE 镜头练习", projectType: "animation", status: "active",
  targetTools: ["Unreal Engine"], versions: ["5.5"], resolutionWidth: 1920, resolutionHeight: 1080, frameRate: 24,
  coverAssetId: null, coverImageId: null, assetCount: 0, unavailableAssetCount: 0, taskCount: 1,
  completedTaskCount: 0, reviewTaskCount: 0, progress: 0, mainBoardId: null,
  createdAt: "2026-01-01", updatedAt: "2026-01-02", lastOpenedAt: null, archivedAt: null,
};
const detail: CreativeProject = {
  ...summary, units: [], assets: [], boards: [], paths: [], tasks: [{
    id: "task-1", projectId: summary.id, unitId: null, title: "完成灯光", description: "", status: "todo",
    priority: "high", dueDate: null, sortOrder: 0, assetIds: [], createdAt: "2026-01-01", updatedAt: "2026-01-01",
  }],
};

describe("ProjectHub", () => {
  beforeEach(() => {
    vi.mocked(api.listProjects).mockResolvedValue([summary]);
    vi.mocked(api.getProject).mockResolvedValue(detail);
    vi.mocked(api.moveProjectTask).mockResolvedValue();
    vi.mocked(api.listReferenceBoards).mockResolvedValue([]);
  });

  it("opens a project and moves a task across the fixed kanban columns", async () => {
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: /任务 1/ }));
    const task = await screen.findByText("完成灯光");
    fireEvent.dragStart(task.closest("article")!);
    const doneColumn = screen.getByText("完成", { selector: "strong" }).closest("section")!;
    fireEvent.drop(doneColumn);
    await waitFor(() => expect(api.moveProjectTask).toHaveBeenCalledWith("task-1", "done", 0));
  });
});

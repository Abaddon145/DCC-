import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { EpicFabDialog, ueVersionRelation } from "./EpicFabDialog";
import type { FabUeOptions, UeProjectTarget } from "../types";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../lib/api", () => ({ api: { getWorkspaceContext: vi.fn(), getFabUeOptions: vi.fn(), listUeProjectTargets: vi.fn(), inspectUeProject: vi.fn(), openFabInEpic: vi.fn() } }));
const project: UeProjectTarget = { projectId: "project-1", pathId: "path-1", projectName: "CG 作品", name: "Desert", path: "C:/CG/Desert.uproject", engineAssociation: "5.5", engineVersion: "5.5", available: true, warnings: [] };
const options: FabUeOptions = { assetId: "asset-1", listingId: "06003f78-9a59-4fb8-abbc-14dc276f0b4a", canonicalUrl: "https://www.fab.com/listings/06003f78-9a59-4fb8-abbc-14dc276f0b4a", epicUrl: "com.epicgames.launcher://ue/Fab/product/06003f78-9a59-4fb8-abbc-14dc276f0b4a", supportedVersions: ["5.4", "5.5", "5.6"], versionStatus: "known", warnings: [] };
const show = (onClose = vi.fn()) => render(<EpicFabDialog assetId="asset-1" title="沙漠环境" onClose={onClose}/>);
describe("Epic Fab assisted project flow", () => {
  beforeEach(() => {
    vi.clearAllMocks(); localStorage.clear();
    vi.mocked(api.getWorkspaceContext).mockResolvedValue({ libraryId: "library-1", enabled: true, value: null });
    vi.mocked(api.getFabUeOptions).mockResolvedValue(options);
    vi.mocked(api.listUeProjectTargets).mockResolvedValue([project]);
    vi.mocked(api.inspectUeProject).mockResolvedValue(project);
    vi.mocked(api.openFabInEpic).mockResolvedValue({ listingId: options.listingId, url: options.epicUrl, opened: true, message: "已打开 Epic，请在其中选择工程与版本；尚未下载或安装。" });
  });
  it("compares numeric major/minor versions, not lexical strings", () => {
    expect(ueVersionRelation("5.10", "5.9")).toBe("newer");
    expect(ueVersionRelation("5.4", "5.5.2")).toBe("older");
    expect(ueVersionRelation("5.5.1", "5.5.2")).toBe("same");
    expect(ueVersionRelation("5.5", "custom-engine-id")).toBe("unknown");
  });
  it("loads official versions, rechecks the project, and opens only the backend's stored listing", async () => {
    show();
    await waitFor(() => expect(screen.getByLabelText("Fab 官方支持版本")).toHaveValue("5.5"));
    expect(api.getFabUeOptions).toHaveBeenCalledWith("asset-1", "library-1");
    fireEvent.click(screen.getByRole("button", { name: "打开 Epic 添加流程" }));
    await waitFor(() => expect(api.openFabInEpic).toHaveBeenCalledWith("asset-1", "library-1"));
    expect(api.inspectUeProject).toHaveBeenCalledWith(project.path, "library-1");
    expect(screen.getByText(/尚未下载或安装/)).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("dcc-epic-add:library-1")!)).toEqual({ path: project.path, version: "5.5" });
  });
  it("allows older packages with a warning, blocks packages newer than the project", async () => {
    show(); await waitFor(() => expect(screen.getByLabelText("Fab 官方支持版本")).toHaveValue("5.5"));
    fireEvent.change(screen.getByLabelText("Fab 官方支持版本"), { target: { value: "5.4" } });
    expect(screen.getByText(/所选素材版本低于工程版本/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开 Epic 添加流程" })).toBeEnabled();
    fireEvent.change(screen.getByLabelText("Fab 官方支持版本"), { target: { value: "5.6" } });
    expect(screen.getByRole("button", { name: "打开 Epic 添加流程" })).toBeDisabled();
  });
  it("does not invent versions when the official response is unavailable", async () => {
    vi.mocked(api.getFabUeOptions).mockResolvedValue({ ...options, supportedVersions: [], versionStatus: "unknown" });
    show(); await waitFor(() => expect(screen.getByText(/官方版本信息不完整/)).toBeInTheDocument());
    expect(screen.getByLabelText("Fab 官方支持版本")).toBeDisabled();
    expect(screen.getByRole("button", { name: "打开 Epic 添加流程" })).toBeEnabled();
  });
  it("blocks invalid source or library errors instead of treating them as missing versions", async () => {
    vi.mocked(api.getFabUeOptions).mockRejectedValue(new Error("Fab 来源网址与保存的商品 ID 不一致"));
    show(); await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(/ID 不一致/));
    expect(screen.getByRole("button", { name: "打开 Epic 添加流程" })).toBeDisabled();
    expect(screen.queryByText(/可重试，或打开 Epic 后手动核对/)).not.toBeInTheDocument();
    expect(api.openFabInEpic).not.toHaveBeenCalled();
  });
  it("only inspects a picked .uproject and clears the dialog on a library switch", async () => {
    const onClose = vi.fn(); vi.mocked(open).mockResolvedValue("C:/CG/Other.uproject");
    vi.mocked(api.inspectUeProject).mockResolvedValue({ ...project, path: "C:/CG/Other.uproject", name: "Other", projectId: null, pathId: null, projectName: null });
    show(onClose); await waitFor(() => expect(screen.getByRole("button", { name: "选择其他 .uproject" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "选择其他 .uproject" }));
    await waitFor(() => expect(screen.getByLabelText("目标 UE 工程")).toHaveValue("C:/CG/Other.uproject"));
    expect(api.inspectUeProject).toHaveBeenCalledWith("C:/CG/Other.uproject", "library-1");
    window.dispatchEvent(new Event("library-location-changed")); expect(onClose).toHaveBeenCalledTimes(1);
  });
  it("never opens Epic when the project changes incompatibly before launch", async () => {
    vi.mocked(api.inspectUeProject).mockResolvedValue({ ...project, engineVersion: "5.3" });
    show(); await waitFor(() => expect(screen.getByLabelText("Fab 官方支持版本")).toHaveValue("5.5"));
    fireEvent.click(screen.getByRole("button", { name: "打开 Epic 添加流程" }));
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(/版本已变更/));
    expect(api.openFabInEpic).not.toHaveBeenCalled();
  });
  it("invalidates an in-flight project check at close intent before it can launch Epic", async () => {
    let finish!: (value: UeProjectTarget) => void;
    vi.mocked(api.inspectUeProject).mockReturnValue(new Promise(resolve => { finish = resolve; }));
    const onClose = vi.fn(); show(onClose);
    await waitFor(() => expect(screen.getByRole("button", { name: "打开 Epic 添加流程" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "打开 Epic 添加流程" }));
    expect(api.inspectUeProject).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "关闭 Epic 添加窗口" }));
    await act(async () => finish(project));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(api.openFabInEpic).not.toHaveBeenCalled();
  });
  it("does not write recent project settings when remember context is disabled", async () => {
    vi.mocked(api.getWorkspaceContext).mockResolvedValue({ libraryId: "library-1", enabled: false, value: null });
    show(); await waitFor(() => expect(screen.getByRole("button", { name: "打开 Epic 添加流程" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "打开 Epic 添加流程" }));
    await waitFor(() => expect(api.openFabInEpic).toHaveBeenCalled());
    expect(localStorage.getItem("dcc-epic-add:library-1")).toBeNull();
  });
});

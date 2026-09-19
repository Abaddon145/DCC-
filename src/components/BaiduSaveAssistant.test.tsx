import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import { BaiduSaveAssistant } from "./BaiduSaveAssistant";

vi.mock("../lib/api", () => ({ api: {
  getBaiduNetdiskSettings: vi.fn(), listBaiduNetdiskFolders: vi.fn(), setBaiduNetdiskDefaultPath: vi.fn(),
  transferBaiduSaveTask: vi.fn(), queryBaiduTransferTask: vi.fn(), openShare: vi.fn(), openExternal: vi.fn()
} }));

const tasks = [{ id: "asset-1", name: "石材", shareUrl: "https://pan.baidu.com/s/demo", extractionCode: "a1b2" }];

describe("BaiduSaveAssistant", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.getBaiduNetdiskSettings).mockResolvedValue({ configured: true, connected: true, accountName: "测试账号", defaultPath: "/素材" });
    vi.mocked(api.listBaiduNetdiskFolders).mockResolvedValue([{ name: "UE", path: "/素材/UE" }]);
    vi.mocked(api.setBaiduNetdiskDefaultPath).mockResolvedValue({ configured: true, connected: true, accountName: "测试账号", defaultPath: "/素材" });
    vi.mocked(api.transferBaiduSaveTask).mockResolvedValue({ assetId: "asset-1", taskId: "task-1", status: "submitted", message: "已提交", savedCount: 0 });
    vi.mocked(api.queryBaiduTransferTask).mockResolvedValue({ assetId: "asset-1", taskId: "task-1", status: "success", message: "转存完成", savedCount: 1 });
  });

  it("loads the configured folder and directly submits selected assets", async () => {
    render(<BaiduSaveAssistant tasks={tasks} selectedCount={1} onClose={vi.fn()} notify={vi.fn()} />);
    expect(await screen.findByText("已连接：测试账号")).toBeInTheDocument();
    await waitFor(() => expect(api.listBaiduNetdiskFolders).toHaveBeenCalledWith("/素材"));
    fireEvent.click(screen.getByRole("button", { name: "全部直接转存" }));
    await waitFor(() => expect(api.transferBaiduSaveTask).toHaveBeenCalledWith("asset-1", "/素材"));
    await waitFor(() => expect(api.queryBaiduTransferTask).toHaveBeenCalledWith("asset-1", "task-1"));
    expect(await screen.findByText("转存完成")).toBeInTheDocument();
  });

  it("stops the queue and explains missing share-service permission", async () => {
    vi.mocked(api.transferBaiduSaveTask).mockRejectedValue("当前百度网盘应用尚未开通“文件分享服务”权限（错误码 13998）");
    render(<BaiduSaveAssistant tasks={[...tasks, { ...tasks[0], id: "asset-2", name: "第二项" }]} selectedCount={2} onClose={vi.fn()} notify={vi.fn()} />);
    await screen.findByText("已连接：测试账号");
    fireEvent.click(screen.getByRole("button", { name: "全部直接转存" }));
    expect(await screen.findByText("当前应用未开通文件分享服务权限")).toBeInTheDocument();
    expect(api.transferBaiduSaveTask).toHaveBeenCalledTimes(1);
  });
});

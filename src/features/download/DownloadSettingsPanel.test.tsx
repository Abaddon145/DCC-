import {render, screen, fireEvent, cleanup} from "@testing-library/react";
import {afterEach, beforeEach, expect, it, vi} from "vitest";
import {DownloadSettingsPanel} from "./DownloadSettingsPanel";
import {api} from "../../lib/api";

vi.mock("@tauri-apps/plugin-dialog", () => ({open: vi.fn()}));
vi.mock("../../lib/api", () => ({api: {
  getDownloadSettings: vi.fn(), getDownloadTools: vi.fn(), getDownloadCookieStatus: vi.fn(),
  saveDownloadSettings: vi.fn(), importDownloadCookies: vi.fn(), deleteDownloadCookies: vi.fn(),
}}));
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getDownloadSettings).mockResolvedValue({directory:"", concurrency:2, proxy:"", cookieSource:"none", defaultQuality:"best"});
  vi.mocked(api.getDownloadTools).mockResolvedValue([{name:"yt-dlp", path:"C:/tools/yt-dlp.exe", version:"2026.08.19", ready:true, message:"可用"}]);
  vi.mocked(api.getDownloadCookieStatus).mockResolvedValue(false);
});
afterEach(cleanup);

it("读取设置时显示等待状态，而不是可反复点击的重新读取", async () => {
  vi.mocked(api.getDownloadSettings).mockReturnValue(new Promise(() => {}));
  render(<DownloadSettingsPanel notify={vi.fn()}/>);
  expect(screen.getByRole("status").textContent).toContain("正在读取");
  expect(screen.queryByRole("button", {name:"重新读取"})).toBeNull();
});

it("工具路径默认折叠，用户可展开查看且保存保留当前字段", async () => {
  render(<DownloadSettingsPanel notify={vi.fn()}/>);
  await screen.findByLabelText("默认下载目录");
  const details = screen.getByText("组件路径").closest("details");
  expect(details).not.toBeNull();
  expect(details?.open).toBe(false);
  fireEvent.change(screen.getByLabelText("代理地址"), {target:{value:"http://127.0.0.1:7890"}});
  fireEvent.click(screen.getByRole("button", {name:"保存下载设置"}));
  await vi.waitFor(() => expect(api.saveDownloadSettings).toHaveBeenCalledWith({directory:"", concurrency:2, proxy:"http://127.0.0.1:7890", cookieSource:"none", defaultQuality:"best"}));
});

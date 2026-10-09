import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { ReliabilitySettings } from "./ReliabilitySettings";
import type { UpgradeBackupStatus } from "../types";

const { invoke, getStatus, saveKeep } = vi.hoisted(() => ({ invoke: vi.fn(), getStatus: vi.fn(), saveKeep: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("../lib/api", () => ({ api: { getUpgradeBackupStatus: getStatus, saveUpgradeBackupKeep: saveKeep } }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
const snapshot: UpgradeBackupStatus = { keep: 5, lastAttemptAt: "2026-10-09T00:00:00Z", result: "success", message: "快照已校验", backupDirectory: "测试库同级/升级前自动备份" };

beforeEach(() => {
  vi.resetAllMocks();
  getStatus.mockResolvedValue(snapshot);
  saveKeep.mockImplementation(async (_id, keep) => ({ ...snapshot, keep }));
  invoke.mockImplementation(async command => command === "get_automatic_backup_policy" ? { enabled: false, directory: "", intervalHours: 24, keep: 3 } : false);
});

it("shows_upgrade_snapshot_separately_from_full_backup", async () => {
  render(<ReliabilitySettings libraryId="library" notify={vi.fn()} />);
  await screen.findByText("升级前数据库保护");
  await waitFor(() => expect(screen.getByLabelText("升级快照保留数量")).toHaveValue(5));
  expect(screen.getByText(/只含数据库，不含媒体/)).toBeInTheDocument();
  expect(screen.getByText("快照已校验")).toBeInTheDocument();
  expect(screen.getByText(/最近尝试/)).toBeInTheDocument();
  expect(screen.getByText("自动完整备份")).toBeInTheDocument();
  expect(screen.getByLabelText("启用自动备份（默认关闭）")).not.toBeChecked();
  expect(screen.getByLabelText("间隔（小时）")).toHaveValue(24);
  expect(screen.getByLabelText("保留自动备份数量")).toHaveValue(3);
  expect(getStatus).toHaveBeenCalledWith("library");
});

it("validates_upgrade_backup_keep_between_one_and_twenty", async () => {
  render(<ReliabilitySettings libraryId="library" notify={vi.fn()} />);
  const input = await screen.findByLabelText("升级快照保留数量");
  const button = screen.getByRole("button", { name: "保存升级保护设置" });
  for (const value of ["", "0", "21", "1.5"]) {
    fireEvent.change(input, { target: { value } });
    expect(button).toBeDisabled();
  }
  expect(saveKeep).not.toHaveBeenCalled();
  fireEvent.change(input, { target: { value: "7" } });
  fireEvent.click(button);
  await waitFor(() => expect(saveKeep).toHaveBeenCalledWith("library", 7));
  expect(invoke.mock.calls.some(call => call[0] === "save_automatic_backup_policy")).toBe(false);
});

it("ignores_status_from_previous_library_and_can_retry_read", async () => {
  let oldReply: (value: UpgradeBackupStatus) => void = () => {};
  getStatus.mockImplementationOnce(() => new Promise<UpgradeBackupStatus>(resolve => { oldReply = resolve; })).mockRejectedValueOnce(new Error("读取失败")).mockResolvedValueOnce({ ...snapshot, keep: 9, message: "新库快照记录" });
  const view = render(<ReliabilitySettings libraryId="old" notify={vi.fn()} />);
  await waitFor(() => expect(getStatus).toHaveBeenCalledWith("old"));
  view.rerender(<ReliabilitySettings libraryId="new" notify={vi.fn()} />);
  const retry = await screen.findByRole("button", { name: "重新读取升级保护状态" });
  await act(async () => oldReply({ ...snapshot, keep: 1, message: "旧库迟到记录" }));
  expect(screen.queryByText("旧库迟到记录")).not.toBeInTheDocument();
  fireEvent.click(retry);
  await screen.findByText("新库快照记录");
  expect(screen.getByLabelText("升级快照保留数量")).toHaveValue(9);
  expect(getStatus).toHaveBeenLastCalledWith("new");
});

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { ModelFileActions } from "./ModelFileActions";

vi.mock("../lib/api", () => ({ api: { openModelLocation: vi.fn(), exportModelPackage: vi.fn() } }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
describe("ModelFileActions", () => {
  beforeEach(() => vi.resetAllMocks());
  it("reveals only the entry ID and recovers its controls on missing files", async () => {
    vi.mocked(api.openModelLocation).mockRejectedValue(new Error("模型原件缺失"));
    const notify = vi.fn();
    render(<ModelFileActions id="model-id" notify={notify} />);
    fireEvent.click(screen.getByRole("button", { name: "在资源管理器中显示" }));
    await waitFor(() => expect(notify).toHaveBeenCalledWith("Error: 模型原件缺失", true));
    expect(api.openModelLocation).toHaveBeenCalledWith("model-id");
    expect(screen.getByRole("button", { name: "导出模型包" })).toBeEnabled();
  });
  it("cancels without exporting and exports to an explicitly chosen directory", async () => {
    vi.mocked(open).mockResolvedValueOnce(null).mockResolvedValueOnce("D:/Project");
    vi.mocked(api.exportModelPackage).mockResolvedValue("D:/Project/Bridge");
    const notify = vi.fn();
    render(<ModelFileActions id="model-id" notify={notify} />);
    fireEvent.click(screen.getByRole("button", { name: "导出模型包" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "导出模型包" })).toBeEnabled());
    expect(api.exportModelPackage).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "导出模型包" }));
    await waitFor(() => expect(api.exportModelPackage).toHaveBeenCalledWith("model-id", "D:/Project"));
    expect(notify).toHaveBeenCalledWith(expect.stringContaining("D:/Project/Bridge"));
  });
});

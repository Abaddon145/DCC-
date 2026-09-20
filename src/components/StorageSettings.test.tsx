import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { StorageSettings } from "./StorageSettings";
import { api } from "../lib/api";

vi.mock("../lib/api", () => ({ api: {
  getLibraryLocations: vi.fn().mockResolvedValue({
    current: { path: "C:\\Library", name: "测试库", libraryId: "12345678-abcd", available: true, isCurrent: true, retainedCopy: false, sizeBytes: 1024, lastOpenedAt: null },
    recent: [], startupWarning: null
  }),
  getTranslationSettings: vi.fn().mockResolvedValue({ provider: "baidu", configured: false, fabAutoTranslate: true, contentLanguage: "zh-CN" }),
  saveTranslationCredentials: vi.fn().mockResolvedValue(undefined),
  testTranslationService: vi.fn().mockResolvedValue({ success: true, message: "连接成功" }),
  listTranslationTerms: vi.fn().mockResolvedValue([]),
  setFabAutoTranslate: vi.fn(), deleteTranslationCredentials: vi.fn(), changeLibrary: vi.fn(), forgetRecentLibrary: vi.fn()
} }));

describe("StorageSettings translation credentials", () => {
  it("enables first-time save-and-test after both credentials are entered", async () => {
    const notify = vi.fn();
    render(<StorageSettings onClose={vi.fn()} onLibraryChanged={vi.fn()} notify={notify} />);
    const testButton = await screen.findByRole("button", { name: "测试连接" });
    expect(testButton).toBeDisabled();
    fireEvent.change(screen.getByLabelText("APP ID"), { target: { value: "demo-app" } });
    fireEvent.change(screen.getByLabelText("密钥"), { target: { value: "demo-secret" } });
    const saveAndTest = screen.getByRole("button", { name: "保存并测试" });
    expect(saveAndTest).toBeEnabled();
    fireEvent.click(saveAndTest);
    await waitFor(() => expect(api.saveTranslationCredentials).toHaveBeenCalledWith("demo-app", "demo-secret"));
    await waitFor(() => expect(api.testTranslationService).toHaveBeenCalled());
    expect(notify).toHaveBeenCalledWith("连接成功");
  });
});

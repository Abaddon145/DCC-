import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import { TranslationServiceOptions } from "./TranslationServiceOptions";
import type { TranslationSettings } from "../types";

vi.mock("../lib/api", () => ({ api: {
  saveDeepseekCredentials: vi.fn(), testTranslationService: vi.fn(), saveTranslationPreferences: vi.fn(),
  deleteDeepseekCredentials: vi.fn(), clearTranslationCache: vi.fn(),
} }));
const settings: TranslationSettings = {
  provider: "deepseek", configured: true, deepseekConfigured: true, baiduConfigured: false,
  contentLanguage: "zh-CN", fabAutoTranslate: true,
  preferences: { providerOrder: ["deepseek", "baidu"], autoFallback: true, cacheEnabled: true, deepseekMonthlyLimit: 1_000_000 },
  usage: { month: "2026-09", deepseekTokens: 250, baiduCharacters: 0, cacheHits: 2, cacheEntries: 10 },
};
describe("TranslationServiceOptions", () => {
  beforeEach(() => vi.resetAllMocks());
  it("shows course usage separately without counting it as translation", () => {
    render(<TranslationServiceOptions settings={{ ...settings, usage: { ...settings.usage!, creativeTokens: 50, courseTokens: 100 } }} reload={vi.fn()} notify={vi.fn()} />);
    const usage = screen.getByText(/DeepSeek 250 Token/);
    expect(usage).toHaveTextContent("创作 50 Token · 课程分析 100 Token · 翻译 100 Token");
  });
  it("saves and clears an entered key before testing the selected provider", async () => {
    vi.mocked(api.saveDeepseekCredentials).mockResolvedValue();
    vi.mocked(api.testTranslationService).mockResolvedValue({ success: true, message: "连接成功" });
    const reload = vi.fn().mockResolvedValue(undefined);
    render(<TranslationServiceOptions settings={settings} reload={reload} notify={vi.fn()} />);
    const key = screen.getByLabelText("DeepSeek API Key");
    expect(key).toHaveValue("");
    fireEvent.change(key, { target: { value: "test-key-not-a-real-secret" } });
    fireEvent.click(screen.getByRole("button", { name: "保存并测试 DeepSeek" }));
    await waitFor(() => expect(api.testTranslationService).toHaveBeenCalledWith("deepseek"));
    expect(key).toHaveValue("");
    expect(reload).toHaveBeenCalled();
  });
  it("validates the local cap and saves priority without exposing credentials", async () => {
    const reload = vi.fn().mockResolvedValue(undefined);
    render(<TranslationServiceOptions settings={settings} reload={reload} notify={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("DeepSeek 本机月度 Token 上限"), { target: { value: "0" } });
    expect(screen.getByRole("button", { name: "保存翻译设置" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("DeepSeek 本机月度 Token 上限"), { target: { value: "2000000" } });
    fireEvent.change(screen.getByLabelText("翻译服务优先级"), { target: { value: "baidu" } });
    fireEvent.click(screen.getByRole("button", { name: "保存翻译设置" }));
    await waitFor(() => expect(api.saveTranslationPreferences).toHaveBeenCalledWith({
      providerOrder: ["baidu", "deepseek"], autoFallback: true, cacheEnabled: true, deepseekMonthlyLimit: 2000000,
    }));
  });
});

import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TranslationCostDialog, combineTranslationCosts } from "./TranslationCostDialog";
import type { TranslationCostEstimate } from "../types";

const estimate: TranslationCostEstimate = { pendingFieldCount: 2, characters: 20, cacheHits: 1, tokensMin: 0, tokensMax: 6000, month: "2026-10", monthUsed: 10, monthLimit: 1000000, wouldExceed: false, onlineRequests: 1, baiduCharacters: 0, rows: [], warnings: [] };
const props = () => ({ estimate, loading: false, error: "", onConfirm: vi.fn().mockResolvedValue(undefined), onCancel: vi.fn(), onRetry: vi.fn(), onRawSave: vi.fn().mockResolvedValue(undefined) });
describe("translation cost confirmation", () => {
  it("combines_chunks_without_counting_monthly_usage_twice", () => {
    const result = combineTranslationCosts([estimate, { ...estimate, monthUsed: 100, monthLimit: 12050 }]);
    expect(result.tokensMax).toBe(12000); expect(result.monthUsed).toBe(100);
    expect(result.wouldExceed).toBe(true);
    expect(() => combineTranslationCosts([estimate, { ...estimate, month: "2026-11" }])).toThrow(/月份/);
    expect(combineTranslationCosts([{ ...estimate, tokensMax: 0, monthUsed: 9999999 }]).wouldExceed).toBe(false);
  });
  it("disables_online_when_estimate_exceeds", () => {
    render(<TranslationCostDialog {...props()} estimate={{ ...estimate, wouldExceed: true }} />);
    expect(screen.getByRole("button", { name: "确认并补译" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "仅保存原文" })).toBeEnabled();
  });
  it("shows_no_online_request_when_disabled", () => {
    render(<TranslationCostDialog {...props()} estimate={{ ...estimate, tokensMax: 0, onlineRequests: 0 }} />);
    expect(screen.getByText(/不会请求在线翻译/)).toBeInTheDocument();
  });
  it("retains_raw_save_after_cost_failure", async () => {
    const callbacks = props();
    render(<TranslationCostDialog {...callbacks} estimate={null} error="缓存暂时不可读" />);
    expect(screen.getByRole("button", { name: "确认并补译" })).toBeDisabled();
    expect(screen.queryByText(/0–/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "仅保存原文" }));
    await act(async () => {});
    expect(callbacks.onRawSave).toHaveBeenCalledTimes(1);
  });
  it("prevents_duplicate_confirmation_submit", async () => {
    const callbacks = props(); let finish!: () => void;
    callbacks.onConfirm.mockImplementation(() => new Promise<void>(resolve => { finish = resolve; }));
    render(<TranslationCostDialog {...callbacks} />);
    const button = screen.getByRole("button", { name: "确认并补译" });
    fireEvent.click(button); fireEvent.click(button);
    expect(callbacks.onConfirm).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "仅保存原文" })).toBeDisabled();
    await act(async () => finish());
  });
});

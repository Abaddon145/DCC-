import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { HealthPanel } from "./HealthPanel";

describe("HealthPanel link checking", () => {
  it("starts a manual scan and exposes invalid links as a separate issue", () => {
    const onCheckLinks = vi.fn();
    const onSelect = vi.fn();
    render(<HealthPanel summary={{ totalIssues: 2, counts: [{ issue: "invalidLink", label: "网盘链接已失效", count: 2 }] }} activeIssue={null} loading={false} linkCheckRunning={false} linkProgress={null} onSelect={onSelect} onRefresh={vi.fn()} onCheckLinks={onCheckLinks} onCancelLinkCheck={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /检查全部网盘链接/ }));
    expect(onCheckLinks).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: /网盘链接已失效/ }));
    expect(onSelect).toHaveBeenCalledWith("invalidLink");
  });

  it("shows progress and allows cancellation", () => {
    const onCancel = vi.fn();
    render(<HealthPanel summary={null} activeIssue={null} loading={false} linkCheckRunning linkProgress={{ checked: 5, total: 10, valid: 3, invalid: 1, error: 1, currentUrl: null }} onSelect={vi.fn()} onRefresh={vi.fn()} onCheckLinks={vi.fn()} onCancelLinkCheck={onCancel} />);
    expect(screen.getByText("5 / 10 · 50%")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /停止检查/ }));
    expect(onCancel).toHaveBeenCalledOnce();
  });
});

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TranslationBatchReportDialog } from "./TranslationBatchReportDialog";

describe("TranslationBatchReportDialog", () => {
  it("reports partial failures and retries only the pending IDs", () => {
    const onRetry = vi.fn();
    render(<TranslationBatchReportDialog busy={false} progress={null} onClose={vi.fn()} onRetry={onRetry} report={{ completed: 1, pending: 1, skipped: 0, rows: [
      { assetId: "complete", name: "Done", status: "complete", messages: [] },
      { assetId: "pending", name: "Needs translation", status: "pending", messages: ["待补译：中文描述"] },
    ] }} />);
    expect(screen.getByText(/双语完成 1/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /重试待补译 1 项/ }));
    expect(onRetry).toHaveBeenCalledWith(["pending"]);
  });
});

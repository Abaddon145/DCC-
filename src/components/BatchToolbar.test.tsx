import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { BatchToolbar } from "./BatchToolbar";

describe("BatchToolbar", () => {
  it("selects the complete filtered result instead of only loaded cards", () => {
    const onSelectAll = vi.fn();
    render(<BatchToolbar count={80} totalCount={20000} categories={[]} onSelectAll={onSelectAll} onClear={vi.fn()} onExit={vi.fn()} onAddToReference={vi.fn()} onSaveToBaidu={vi.fn()} onApply={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "全选当前结果 20000 项" }));
    expect(onSelectAll).toHaveBeenCalledOnce();
  });
});

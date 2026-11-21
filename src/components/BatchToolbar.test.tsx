import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { BatchToolbar } from "./BatchToolbar";

function renderToolbar(onApply = vi.fn()) {
  render(<BatchToolbar count={2} totalCount={12} categories={[{ id: "cat", name: "环境", parentId: null, sortOrder: 0, assetCount: 1 }]} onSelectAll={vi.fn()} onClear={vi.fn()} onExit={vi.fn()} onAddToReference={vi.fn()} onTranslate={vi.fn()} onApply={onApply} />);
  return onApply;
}

describe("BatchToolbar", () => {
  it("selects the complete filtered result instead of only loaded cards", () => {
    const onSelectAll = vi.fn();
    render(<BatchToolbar count={80} totalCount={20000} categories={[]} onSelectAll={onSelectAll} onClear={vi.fn()} onExit={vi.fn()} onAddToReference={vi.fn()} onTranslate={vi.fn()} onApply={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "全选当前结果 20000 项" }));
    expect(onSelectAll).toHaveBeenCalledOnce();
  });

  it("offers manual bilingual completion only for explicitly selected assets", () => {
    const onTranslate = vi.fn();
    const { rerender } = render(<BatchToolbar count={0} totalCount={12} categories={[]} onSelectAll={vi.fn()} onClear={vi.fn()} onExit={vi.fn()} onAddToReference={vi.fn()} onTranslate={onTranslate} onApply={vi.fn()} />);
    expect(screen.getByRole("button", { name: "补齐双语" })).toBeDisabled();
    rerender(<BatchToolbar count={2} totalCount={12} categories={[]} onSelectAll={vi.fn()} onClear={vi.fn()} onExit={vi.fn()} onAddToReference={vi.fn()} onTranslate={onTranslate} onApply={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "补齐双语" }));
    expect(onTranslate).toHaveBeenCalledOnce();
  });

  it("keeps apply unavailable when every batch control is unchanged", () => {
    const onApply = renderToolbar();
    const apply = screen.getByRole("button", { name: "应用" });
    expect(apply).toBeDisabled();
    expect(apply).toHaveAttribute("aria-describedby", "batch-apply-reason");
    expect(screen.getByRole("status")).toHaveTextContent("没有要应用的更改");
    fireEvent.change(screen.getByLabelText("添加标签"), { target: { value: " ; " } });
    fireEvent.click(apply);
    expect(onApply).not.toHaveBeenCalled();
  });

  it("submits only the controls the user changed", async () => {
    const onApply = renderToolbar(vi.fn().mockResolvedValue("updated"));
    fireEvent.change(screen.getByLabelText("批量分类"), { target: { value: "set:cat" } });
    fireEvent.change(screen.getByLabelText("添加标签"), { target: { value: "岩石" } });
    fireEvent.click(screen.getByRole("button", { name: "应用" }));
    expect(onApply).toHaveBeenCalledWith({ categoryId: "cat", clearCategory: false, addTags: ["岩石"], removeTags: [], favorite: null });
    expect(screen.queryByText("没有要应用的更改")).toBeNull();
  });

  it("shows an identical-value reason separately from an illegal request", async () => {
    const onApply = renderToolbar(vi.fn().mockResolvedValueOnce("identical").mockRejectedValueOnce(new Error("目标分类不存在")));
    fireEvent.change(screen.getByLabelText("批量收藏"), { target: { value: "true" } });
    fireEvent.click(screen.getByRole("button", { name: "应用" }));
    expect(await screen.findByRole("status")).toHaveTextContent("所选操作与当前值相同，不会更新记录");
    expect(screen.getByRole("button", { name: "应用" })).toBeDisabled();
    expect(screen.queryByText("没有要应用的更改")).toBeNull();
    fireEvent.change(screen.getByLabelText("批量收藏"), { target: { value: "false" } });
    fireEvent.click(screen.getByRole("button", { name: "应用" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("目标分类不存在");
    expect(screen.queryByText("没有要应用的更改")).toBeNull();
    expect(screen.queryByText("所选操作与当前值相同，不会更新记录")).toBeNull();
  });
});

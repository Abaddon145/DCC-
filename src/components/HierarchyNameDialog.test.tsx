import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { HierarchyNameDialog } from "./HierarchyNameDialog";

describe("HierarchyNameDialog", () => {
  it.each(["分类", "文件夹"] as const)("uses the same in-app naming flow for %s", kind => {
    const onNameChange = vi.fn();
    const onSubmit = vi.fn();
    const { rerender } = render(<HierarchyNameDialog kind={kind} editing={false} name="" busy={false} onNameChange={onNameChange} onClose={vi.fn()} onSubmit={onSubmit} />);
    expect(screen.getByRole("button", { name: `创建${kind}` })).toBeDisabled();
    fireEvent.change(screen.getByRole("textbox", { name: `${kind}名称` }), { target: { value: "环境" } });
    expect(onNameChange).toHaveBeenCalledWith("环境");
    rerender(<HierarchyNameDialog kind={kind} editing={false} name="环境" busy={false} onNameChange={onNameChange} onClose={vi.fn()} onSubmit={onSubmit} />);
    fireEvent.click(screen.getByRole("button", { name: `创建${kind}` }));
    expect(onSubmit).toHaveBeenCalledOnce();
  });
});

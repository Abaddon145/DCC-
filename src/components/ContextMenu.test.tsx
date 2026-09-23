import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ContextMenu, menuPosition } from "./ContextMenu";

describe("ContextMenu", () => {
  it("keeps its menu inside the viewport", () => {
    expect(menuPosition(790, 590, 180, 120, 800, 600)).toEqual({ left: 612, top: 472 });
    expect(menuPosition(-20, -10, 180, 120, 800, 600)).toEqual({ left: 8, top: 8 });
  });

  it("runs an action once and closes, while disabling unavailable actions", () => {
    const run = vi.fn();
    const close = vi.fn();
    render(<ContextMenu x={20} y={30} title="项目" onClose={close} items={[{ label: "打开", run }, { label: "永久删除", run, disabled: true, danger: true }]} />);
    fireEvent.click(screen.getByRole("menuitem", { name: "打开" }));
    expect(close).toHaveBeenCalledOnce();
    expect(run).toHaveBeenCalledOnce();
    expect(screen.getByRole("menuitem", { name: "永久删除" })).toBeDisabled();
  });

  it("closes on Escape or outside pointerdown", () => {
    const close = vi.fn();
    render(<ContextMenu x={20} y={30} onClose={close} items={[{ label: "打开", run: vi.fn() }]} />);
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.pointerDown(document.body);
    expect(close).toHaveBeenCalledTimes(2);
  });
});

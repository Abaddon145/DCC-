import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SearchHelpPopover } from "./SearchHelpPopover";

describe("SearchHelpPopover", () => {
  it("keeps the help open while the pointer crosses to the panel and closes after leaving both", () => {
    vi.useFakeTimers();
    render(<SearchHelpPopover />);
    const trigger = screen.getByRole("button", { name: "查看高级搜索帮助" });
    fireEvent.mouseEnter(trigger);
    const panel = screen.getByRole("dialog", { name: "高级搜索帮助" });
    fireEvent.mouseLeave(trigger);
    fireEvent.mouseEnter(panel);
    act(() => vi.advanceTimersByTime(200));
    expect(screen.getByRole("dialog", { name: "高级搜索帮助" })).toBeTruthy();
    fireEvent.mouseLeave(panel);
    act(() => vi.advanceTimersByTime(200));
    expect(screen.queryByRole("dialog", { name: "高级搜索帮助" })).toBeNull();
    vi.useRealTimers();
  });

  it("closes on Escape and returns focus to the trigger", () => {
    render(<SearchHelpPopover />);
    const trigger = screen.getByRole("button", { name: "查看高级搜索帮助" });
    fireEvent.click(trigger);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "高级搜索帮助" })).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });
});

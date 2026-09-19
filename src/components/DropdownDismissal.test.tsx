import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CategoryTree } from "./CategoryTree";
import { FilterBar } from "./FilterBar";
import type { FilterOptions, SearchRequest } from "../types";

const request: SearchRequest = {
  query: "",
  categoryIds: [],
  tags: [],
  dccTools: [],
  versions: [],
  formats: [],
  licenses: [],
  favoriteOnly: false,
  recentOnly: false,
  sort: "updated",
  offset: 0,
  limit: 50,
  contentLanguage: "zh-CN"
};

const options: FilterOptions = {
  tags: ["环境"],
  dccTools: ["Unreal Engine"],
  versions: ["5.5"],
  formats: ["uasset"],
  licenses: ["Fab 标准许可"]
};

describe("dropdown pointer dismissal", () => {
  it("keeps a filter open while crossing into its options and closes outside the whole menu", () => {
    vi.useFakeTimers();
    const { container } = render(<FilterBar request={request} options={options} onChange={vi.fn()} />);
    const details = container.querySelector<HTMLDetailsElement>(".filter-menu")!;
    const summary = screen.getByText("标签").closest("summary")!;

    fireEvent.click(summary);
    expect(details.open).toBe(true);
    fireEvent.mouseLeave(details);
    fireEvent.mouseEnter(details);
    act(() => vi.advanceTimersByTime(200));
    expect(details.open).toBe(true);
    fireEvent.mouseLeave(details);
    act(() => vi.advanceTimersByTime(200));
    expect(details.open).toBe(false);
    vi.useRealTimers();
  });

  it("closes a category action menu and does not reopen on hover", () => {
    vi.useFakeTimers();
    const { container } = render(<CategoryTree
      categories={[{ id: "category-1", name: "角色", parentId: null, sortOrder: 0, assetCount: 1 }]}
      selected={[]}
      total={1}
      onChange={vi.fn()}
      onAdd={vi.fn()}
      onRename={vi.fn()}
      onDelete={vi.fn()}
      activeDrag={null}
      onPointerDragStart={vi.fn()}
    />);
    const details = container.querySelector<HTMLDetailsElement>(".row-menu")!;
    const summary = details.querySelector("summary")!;

    fireEvent.click(summary);
    expect(details.open).toBe(true);
    fireEvent.mouseLeave(details);
    act(() => vi.advanceTimersByTime(200));
    expect(details.open).toBe(false);
    fireEvent.mouseEnter(details.closest(".category-row")!);
    expect(details.open).toBe(false);
    vi.useRealTimers();
  });

  it("uses the same whole-menu leave behavior for sorting", () => {
    vi.useFakeTimers();
    const { container } = render(<FilterBar request={request} options={options} onChange={vi.fn()} />);
    const details = container.querySelector<HTMLDetailsElement>(".sort-menu")!;
    fireEvent.click(details.querySelector("summary")!);
    expect(details.open).toBe(true);
    fireEvent.mouseLeave(details);
    act(() => vi.advanceTimersByTime(200));
    expect(details.open).toBe(false);
    vi.useRealTimers();
  });
});

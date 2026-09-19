import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CategoryTree, categoryDropZone, categoryMoveRequest } from "./CategoryTree";
import type { Category, LibraryDragPayload } from "../types";

const categories: Category[] = [
  { id: "a", name: "A", parentId: null, sortOrder: 0, assetCount: 0 },
  { id: "b", name: "B", parentId: null, sortOrder: 1, assetCount: 0 },
  { id: "a1", name: "A1", parentId: "a", sortOrder: 0, assetCount: 0 },
  { id: "a2", name: "A2", parentId: "a", sortOrder: 1, assetCount: 0 }
];

function renderTree(activeDrag: LibraryDragPayload, overrides: Record<string, unknown> = {}) {
  const props = {
    categories, selected: [], total: 0, activeDrag,
    onChange: vi.fn(), onAdd: vi.fn(), onRename: vi.fn(), onDelete: vi.fn(), onPointerDragStart: vi.fn(),
    ...overrides
  };
  return { ...render(<CategoryTree {...props} />), props };
}

describe("category drag organization", () => {
  it("computes nesting, sibling order, root moves, and rejects cycles", () => {
    expect(categoryMoveRequest(categories, "b", "a", "inside")).toEqual({ id: "b", targetParentId: "a", targetIndex: 2 });
    expect(categoryMoveRequest(categories, "a2", "a1", "before")).toEqual({ id: "a2", targetParentId: "a", targetIndex: 0 });
    expect(categoryMoveRequest(categories, "a1", null, "inside")).toEqual({ id: "a1", targetParentId: null, targetIndex: 2 });
    expect(categoryMoveRequest(categories, "a", "a1", "inside")).toBeNull();
  });

  it("highlights a category as the asset drop target", () => {
    renderTree({ kind: "assets", ids: ["x", "y"], label: "2 项素材" });
    const row = screen.getByRole("button", { name: /B/ }).closest(".category-row")!;
    fireEvent.pointerMove(row, { clientY: 10 });
    expect(row).toHaveClass("drag-inside");
  });

  it("marks the all-assets row as a valid root drop target", () => {
    const { container } = renderTree({ kind: "assets", ids: ["x"], label: "素材" });
    const root = container.querySelector(".category-row.root")!;
    fireEvent.pointerMove(root, { clientY: 10 });
    expect(root).toHaveClass("drag-inside");
    expect(root).toHaveAttribute("data-category-drop-id", "__root__");
  });

  it("starts a pointer drag from a category and computes edge zones", () => {
    const onPointerDragStart = vi.fn();
    renderTree({ kind: "assets", ids: ["x"], label: "素材" }, { onPointerDragStart });
    fireEvent.pointerDown(screen.getByRole("button", { name: /B/ }), { button: 0, pointerId: 2, clientX: 10, clientY: 10 });
    expect(onPointerDragStart).toHaveBeenCalledWith({ kind: "category", id: "b", label: "B" }, expect.anything());
    const payload: LibraryDragPayload = { kind: "category", id: "b", label: "B" };
    expect(categoryDropZone({ top: 100, height: 100 }, 110, payload)).toBe("before");
    expect(categoryDropZone({ top: 100, height: 100 }, 150, payload)).toBe("inside");
    expect(categoryDropZone({ top: 100, height: 100 }, 190, payload)).toBe("after");
  });

  it("expands a collapsed category after hovering for 600ms", () => {
    vi.useFakeTimers();
    renderTree({ kind: "assets", ids: ["x"], label: "素材" });
    const parentRow = screen.getByRole("button", { name: /^A0$/ }).closest(".category-row")!;
    fireEvent.click(parentRow.querySelector(".tree-toggle")!);
    expect(screen.queryByRole("button", { name: /^A10$/ })).not.toBeInTheDocument();
    fireEvent.pointerMove(parentRow, { clientY: 10 });
    act(() => vi.advanceTimersByTime(600));
    expect(screen.getByRole("button", { name: /^A10$/ })).toBeInTheDocument();
    vi.useRealTimers();
  });
});

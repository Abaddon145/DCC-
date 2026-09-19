import { describe, expect, it } from "vitest";
import type { ReferenceBoardItem } from "../types";
import { arrangeItems, fitBounds, itemBounds, reorderItems, screenToBoard, visibleItems, zoomViewAt } from "./referenceBoard";

const item = (id: string, x: number, y: number, zIndex: number): ReferenceBoardItem => ({ id, boardId: "b", originalName: `${id}.png`, pixelWidth: 100, pixelHeight: 50, x, y, width: 100, height: 50, rotation: 0, zIndex, sourceAssetId: null, sourceImageId: null });

describe("reference board geometry", () => {
  it("keeps the world point under the cursor while zooming", () => {
    const view = { x: 20, y: 30, scale: 1 };
    const cursor = { x: 220, y: 130 };
    const world = screenToBoard(cursor, view);
    const next = zoomViewAt(view, cursor, 2);
    expect(screenToBoard(cursor, next)).toEqual(world);
  });

  it("computes rotated bounds and a fitting view", () => {
    const rotated = { ...item("a", 0, 0, 0), width: 100, height: 100, rotation: 45 };
    const bounds = itemBounds(rotated);
    expect(bounds.right - bounds.left).toBeCloseTo(Math.sqrt(2) * 100);
    expect(fitBounds(bounds, { width: 800, height: 600 }).scale).toBeGreaterThan(1);
  });

  it("arranges and reorders selected items deterministically", () => {
    const items = [item("a", 20, 20, 0), item("b", 30, 30, 1), item("c", 40, 40, 2)];
    const arranged = arrangeItems(items, new Set(["a", "b"]));
    expect(arranged[0].x).not.toBe(arranged[1].x);
    const front = reorderItems(items, new Set(["a"]), "front");
    expect(front.at(-1)?.id).toBe("a");
  });

  it("culls a 1000-image board to the current viewport", () => {
    const items = Array.from({ length: 1000 }, (_, index) => item(String(index), index * 500, 0, index));
    const visible = visibleItems(items, { x: 0, y: 0, scale: 1 }, { width: 1000, height: 600 }, 0);
    expect(visible.map(value => value.id)).toEqual(["0", "1", "2"]);
  });
});

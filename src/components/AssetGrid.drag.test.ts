import { describe, expect, it } from "vitest";
import { assetIdsForDrag } from "./AssetGrid";

describe("asset card drag selection", () => {
  it("moves all selected assets only when dragging an already selected card", () => {
    const selected = new Set(["a", "b"]);
    expect(new Set(assetIdsForDrag("a", true, selected))).toEqual(selected);
    expect(assetIdsForDrag("c", true, selected)).toEqual(["c"]);
    expect(assetIdsForDrag("a", false, selected)).toEqual(["a"]);
  });
});

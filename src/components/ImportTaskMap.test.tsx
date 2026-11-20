import { readFileSync } from "node:fs";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ImportTaskMap, importTasks } from "./ImportTaskMap";

describe("ImportTaskMap", () => {
  it("names the current task and keeps the full map closed until asked", () => {
    render(<ImportTaskMap current="media" />);
    expect(screen.getByRole("region", { name: "导入任务对照" }).textContent).toContain("导入文件");
    expect(screen.getByRole("region", { name: "导入任务对照" }).textContent).toContain("模型不走全部资产扫描");
    expect(screen.queryByRole("list", { name: "全部导入任务" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "查看全部导入任务" }));
    const items = screen.getAllByRole("listitem");
    expect(items).toHaveLength(importTasks.length);
    expect(items.find(item => item.getAttribute("aria-current") === "true")?.textContent).toContain("导入文件");
    expect(screen.getByText(/各入口仍分开执行/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "收起导入任务对照" }));
    expect(screen.queryByRole("list", { name: "全部导入任务" })).toBeNull();
  });

  it("keeps the scan task explicit that scanning does not ingest", () => {
    render(<ImportTaskMap current="scan" initiallyOpen />);
    const current = screen.getByRole("listitem", { current: true });
    expect(current.textContent).toContain("扫描 / 导入文件");
    expect(current.textContent).toContain("不会自动入库");
    expect(current.textContent).toContain("不移动源文件");
    expect(screen.getByText("批量导入")).toBeTruthy();
    expect(screen.getByText("快速录入")).toBeTruthy();
  });
});

describe("reduced motion still covers the new feedback", () => {
  it("leaves the existing blanket reduced-motion rules in place", () => {
    const experience = readFileSync("src/components/Experience.css", "utf8");
    const styles = readFileSync("src/styles.css", "utf8");
    const assets = readFileSync("src/features/assets/assets.css", "utf8");
    expect(experience).toMatch(/@media\(prefers-reduced-motion:reduce\)[\s\S]*animation:none!important/);
    expect(experience).toMatch(/@media\(prefers-reduced-motion:reduce\)[\s\S]*transition:none!important/);
    expect(styles).toContain(':root[data-reduce-motion="true"]');
    expect(styles).toMatch(/data-reduce-motion="true"[\s\S]*animation:none!important/);
    expect(assets).toMatch(/prefers-reduced-motion:reduce[\s\S]*core-scope-stage/);
    expect(assets).toMatch(/data-reduce-motion="true"[\s\S]*core-scope-stage/);
  });
});

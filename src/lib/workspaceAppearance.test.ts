import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

const stylesheet = readFileSync("src/styles.css", "utf8");
const style = document.createElement("style");

function luminance(hex: string) {
  expect(hex).toMatch(/^#[\da-f]{6}$/i);
  const [r, g, b] = hex.slice(1).match(/../g)!.map(channel => {
    const value = parseInt(channel, 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

afterEach(() => {
  style.remove();
  delete document.documentElement.dataset.theme;
});

describe("workspace text contrast", () => {
  it("does not apply sidebar navigation widths to task dialog actions", () => {
    style.textContent = stylesheet;
    document.head.append(style);
    const footer = document.createElement("div");
    footer.className = "sidebar-footer";
    footer.innerHTML = '<button>任务中心</button><section class="task-center modal-card"><header><div><h2>任务中心</h2></div><button class="icon-button">关闭</button></header><footer><button class="secondary-button">收起</button></footer></section>';
    document.body.append(footer);
    try {
      expect(getComputedStyle(footer.querySelector("button")!).width).toBe("100%");
      for (const action of footer.querySelectorAll(".task-center button")) {
        expect(getComputedStyle(action).width).not.toBe("100%");
      }
    } finally {
      footer.remove();
    }
  });

  it("keeps media cards within their virtual row height at larger UI font sizes", () => {
    style.textContent = stylesheet;
    document.head.append(style);
    const rows = document.createElement("div");
    rows.style.setProperty("--font-scale", "1.25");
    rows.innerHTML = '<div class="media-virtual-row grid"><article>预览</article></div><div class="media-virtual-row list"><article>预览</article></div>';
    document.body.append(rows);
    try {
      expect(getComputedStyle(rows.querySelector(".grid article")!).height).toBe("226px");
      expect(getComputedStyle(rows.querySelector(".list article")!).height).toBe("76px");
    } finally {
      rows.remove();
    }
  });

  it("keeps import and quick modal bodies scrollable so modal footers are not clipped", () => {
    style.textContent = stylesheet;
    document.head.append(style);
    const modal = document.createElement("section");
    modal.className = "import-modal";
    modal.innerHTML = '<div class="import-body fab-library-body"></div><div class="quick-body"></div><footer class="modal-footer"></footer>';
    document.body.append(modal);
    try {
      for (const selector of [".import-body.fab-library-body", ".quick-body"]) {
        const computed = getComputedStyle(modal.querySelector(selector)!);
        expect(computed.minHeight).toBe("0px");
        expect(computed.flexGrow).toBe("1");
        expect(computed.overflowY).toBe("auto");
      }
      expect(getComputedStyle(modal.querySelector(".modal-footer")!).flexShrink).toBe("0");
    } finally {
      modal.remove();
    }
  });

  it.each(["graphite", "ue-slate", "midnight"])("keeps small metadata readable in %s", theme => {
    style.textContent = stylesheet;
    document.head.append(style);
    document.documentElement.dataset.theme = theme;
    const tokens = getComputedStyle(document.documentElement);
    for (const foreground of ["--muted", "--muted-2", "--text-secondary"]) {
      for (const background of ["--bg", "--panel", "--panel-2", "--panel-3"]) {
        const a = luminance(tokens.getPropertyValue(foreground).trim());
        const b = luminance(tokens.getPropertyValue(background).trim());
        const contrast = (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
        expect(contrast, `${theme}: ${foreground} on ${background}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });
});

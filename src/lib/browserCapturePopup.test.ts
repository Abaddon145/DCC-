import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import { fireEvent, waitFor } from "@testing-library/dom";

afterEach(() => { document.body.replaceChildren(); document.head.replaceChildren(); vi.unstubAllGlobals(); vi.resetModules(); });

async function popup() {
  const html = readFileSync("browser-extension/popup.html", "utf8");
  document.body.innerHTML = html.slice(html.indexOf("<body>") + 6, html.indexOf("</body>"));
  const style = document.createElement("style");
  style.textContent = readFileSync("browser-extension/popup.css", "utf8"); document.head.append(style);
  let connected = false;
  const sendMessage = vi.fn(async ({ action }: { action: string }) => action === "open" || connected
    ? { ok: true, value: action === "open" ? {} : { started: false, connection: { name: "测试素材库", libraryId: "library", enabled: true, folders: [], projects: [] } } }
    : { ok: false, error: "Specified native messaging host not found." });
  vi.stubGlobal("chrome", { runtime: { id: "a".repeat(32), sendMessage }, storage: {
    onChanged: { addListener: vi.fn() }, local: { get: async () => ({}) }, session: { get: async () => ({}) },
  } });
  const path = "../../browser-extension/popup.js"; await import(path);
  await waitFor(() => expect(document.getElementById("connection")).toHaveTextContent("尚未连接"));
  return { connect: () => { connected = true; }, sendMessage };
}

it("prevents a narrow initial browser popup from collapsing the document width", async () => {
  await popup();
  expect(parseInt(getComputedStyle(document.documentElement).minWidth)).toBeGreaterThanOrEqual(420);
  expect(getComputedStyle(document.body).maxWidth).not.toBe("100vw");
});

it("exposes setup on a missing host and reconnects without closing the popup", async () => {
  const state = await popup();
  expect(document.querySelector("details")).toHaveAttribute("open");
  expect(document.getElementById("status")).toHaveTextContent("连接浏览器");
  state.connect();
  fireEvent.click(document.getElementById("reconnect")!);
  await waitFor(() => expect(document.getElementById("connection")).toHaveTextContent("测试素材库"));
});

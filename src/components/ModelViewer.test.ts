import { describe, expect, it } from "vitest";
import { vi } from "vitest";
import { ImageBitmapLoader } from "three";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import tauriConfig from "../../src-tauri/tauri.conf.json";
import { objMaterialPaths, resolveModelResource } from "./ModelViewer";

describe("model material resources", () => {
  const modelUrl = "http://dcc-media.localhost/library/bundle/model-id/scene.obj";

  it("finds OBJ material libraries and keeps texture requests inside the model bundle", () => {
    const body = "mtllib textures/stone materials.mtl\nv 0 0 0\nmtllib props.mtl\n";
    expect(objMaterialPaths(body)).toEqual(["textures/stone materials.mtl", "props.mtl"]);
    expect(resolveModelResource(modelUrl, "textures/stone materials.mtl"))
      .toBe("http://dcc-media.localhost/library/bundle/model-id/textures/stone%20materials.mtl");
  });

  it("rejects remote, absolute and parent-directory material references", () => {
    expect(resolveModelResource(modelUrl, "../other/secret.mtl")).toBeNull();
    expect(resolveModelResource(modelUrl, "/secret.mtl")).toBeNull();
    expect(resolveModelResource(modelUrl, "https://example.com/remote.mtl")).toBeNull();
    expect(resolveModelResource(modelUrl, "data:text/plain,secret")).toBeNull();
  });
});

describe("embedded GLB textures and compact model controls", () => {
  it("allows Three.js ImageBitmapLoader to fetch embedded blob textures under the app CSP", async () => {
    const connectSrc = tauriConfig.app.security.csp.split(";").find(rule => rule.trim().startsWith("connect-src")) || "";
    vi.stubGlobal("fetch", vi.fn(async (url: string) => {
      if (url.startsWith("blob:") && !connectSrc.split(/\s+/).includes("blob:")) throw new TypeError("CSP blocked embedded texture fetch");
      return { blob: async () => new Blob([new Uint8Array([1])], { type: "image/png" }) };
    }));
    vi.stubGlobal("createImageBitmap", vi.fn(async () => ({ width: 1, height: 1 })));
    try {
      await expect(new ImageBitmapLoader().loadAsync("blob:embedded-model-texture")).resolves.toMatchObject({ width: 1, height: 1 });
    } finally { vi.unstubAllGlobals(); }
  });

  it("keeps all model action buttons outside the canvas and prevents narrow wrapping", () => {
    const styles = readFileSync(resolve(process.cwd(), "src/styles.css"), "utf8");
    expect(styles).toMatch(/\.model-viewer-shell\s*\{[^}]*display:\s*flex[^}]*flex-direction:\s*column/s);
    expect(styles).toMatch(/\.model-viewer-tools\s*\{[^}]*position:\s*relative[^}]*flex-wrap:\s*wrap/s);
  });
});

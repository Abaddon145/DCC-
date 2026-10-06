// @vitest-environment node
import { expect, it } from "vitest";
import { resolveConfig } from "vite";

it("does not watch generated binaries or disposable tool projects", async () => {
  const config = await resolveConfig({ configFile: "vite.config.ts", configLoader: "runner" }, "serve");
  for (const directory of ["src-tauri/target", "release-artifacts", ".tools"]) {
    expect(config.server.watch?.ignored).toContain(`**/${directory}/**`);
  }
});

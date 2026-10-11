import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = process.cwd();
const read = (name: string) => existsSync(resolve(root, name)) ? readFileSync(resolve(root, name), "utf8") : "";

describe("release documentation", () => {
  it("asset_core_release_versions_are_consistent", () => {
    expect(JSON.parse(read("package.json")).version).toBe("0.22.0");
    const lock=JSON.parse(read("package-lock.json"));expect(lock.version).toBe("0.22.0");expect(lock.packages[""].version).toBe("0.22.0");
    expect(JSON.parse(read("src-tauri/tauri.conf.json")).version).toBe("0.22.0");
    expect(read("src-tauri/Cargo.toml").match(/^version = "([\d.]+)"/m)?.[1]).toBe("0.22.0");
    expect(read("src-tauri/Cargo.lock").match(/name = "dcc-asset-library"\r?\nversion = "([\d.]+)"/)?.[1]).toBe("0.22.0");
  });
  it("changelog_covers_released_versions_in_descending_order", () => {
    const headings = [...read("CHANGELOG.md").matchAll(/^## (v[\d.]+)/gm)].map(match => match[1]);
    expect(headings).toEqual(["v0.22.0", "v0.21.0", "v0.20.6", "v0.20.5", "v0.20.4", "v0.20.3", "v0.20.2", "v0.20.1"]);
  });

  it("readme_has_latest_version_summary_and_valid_changelog_link", () => {
    const readme = read("README.md");
    expect([...readme.matchAll(/^#{2,3} (v[\d.]+)/gm)].map(match => match[1])).toEqual(["v0.22.0"]);
    const target = readme.match(/\[[^\]]+\]\((CHANGELOG\.md)\)/)?.[1];
    expect(target).toBe("CHANGELOG.md");
    expect(existsSync(resolve(root, target!))).toBe(true);
  });

  it("changelog_excludes_installer_and_checksum_details", () => {
    const changelog = read("CHANGELOG.md");
    expect(changelog.length).toBeGreaterThan(0);
    expect(changelog).not.toMatch(/setup\.exe|SHA-?256|[a-f\d]{64}|\d[\d,]+ 字节/i);
    expect([...changelog.matchAll(/^### (.+)$/gm)].every(match => ["新增", "修复", "变更"].includes(match[1]))).toBe(true);
  });
});

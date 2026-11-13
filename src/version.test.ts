import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { APP_VERSION } from "./version";
import { resolvedTable } from "./i18n";
import { en } from "./i18n/en";
import { zhCN } from "./i18n/zh-CN";

// The version used to be written down in five places: the workspace manifest,
// the npm manifest, the Tauri manifest and one line in each message table. A
// release that forgot one of them shipped an app that claimed to be a version it
// was not, and the updater compared against a number nobody could reproduce.
//
// The build injects the number from `package.json` now, and these assertions keep
// the other places honest.

const root = fileURLToPath(new URL("../", import.meta.url));

function readJson(path: string): Record<string, unknown> {
  return JSON.parse(readFileSync(join(root, path), "utf8")) as Record<string, unknown>;
}

describe("the version", () => {
  it("is the one in package.json", () => {
    expect(APP_VERSION).toBe(readJson("package.json").version);
  });

  it("is the same number in every manifest that writes one down", () => {
    expect(readJson("src-tauri/tauri.conf.json").version).toBe(APP_VERSION);

    const cargo = readFileSync(join(root, "Cargo.toml"), "utf8");
    const workspaceVersion = cargo.match(/^version = "([^"]+)"/m)?.[1];
    expect(workspaceVersion).toBe(APP_VERSION);
  });

  it("reaches the interface without a table writing it down", () => {
    expect(resolvedTable("en", "macos").app.versionNote).toBe(`v${APP_VERSION}`);
    expect(resolvedTable("zh-CN", "macos").app.versionNote).toBe(`v${APP_VERSION}`);
  });

  it("is not written into a message table", () => {
    // A number in a table is the drift this whole mechanism exists to remove.
    expect(en.app.versionNote).not.toMatch(/\d+\.\d+\.\d+/);
    expect(zhCN.app.versionNote).not.toMatch(/\d+\.\d+\.\d+/);
  });
});

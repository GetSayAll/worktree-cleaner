// The manifest the updater reads is written by a script rather than by hand, and
// a mistake in it is invisible until an install fails on a user's machine. So the
// script is tested here: what it copies, what it skips, and what it writes.
//
// The test lives next to the script it covers and is written in the same
// language, because the script is tooling rather than part of the application.

import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { buildManifest, collectArtifacts, platformKeys } from "./release-assets.mjs";

const temporaries = [];

function scratch() {
  const directory = mkdtempSync(join(tmpdir(), "release-assets-"));
  temporaries.push(directory);
  return directory;
}

afterEach(() => {
  while (temporaries.length > 0) {
    rmSync(temporaries.pop(), { recursive: true, force: true });
  }
});

describe("the release assets", () => {
  it("answers for both architectures when the macOS build is universal", () => {
    expect(platformKeys("darwin", "universal")).toEqual(["darwin-aarch64", "darwin-x86_64"]);
    expect(platformKeys("darwin", "aarch64")).toEqual(["darwin-aarch64"]);
    expect(platformKeys("windows", "x86_64")).toEqual(["windows-x86_64"]);
  });

  it("copies the installer and the updater archive, each with its signature", () => {
    const bundle = scratch();
    const out = scratch();
    mkdirSync(join(bundle, "macos"), { recursive: true });
    mkdirSync(join(bundle, "dmg"), { recursive: true });
    writeFileSync(join(bundle, "macos", "Worktree Cleaner.app.tar.gz"), "archive");
    writeFileSync(join(bundle, "macos", "Worktree Cleaner.app.tar.gz.sig"), "signature");
    writeFileSync(join(bundle, "dmg", "Worktree Cleaner_0.1.0_universal.dmg"), "image");

    const copied = collectArtifacts({
      bundleDir: bundle,
      outDir: out,
      version: "0.1.0",
      platform: "darwin",
      arch: "universal"
    });

    expect(copied).toHaveLength(3);
    expect(readFileSync(join(out, "Worktree-Cleaner_0.1.0_macos_universal.app.tar.gz"), "utf8")).toBe("archive");
    expect(readFileSync(join(out, "Worktree-Cleaner_0.1.0_macos_universal.app.tar.gz.sig"), "utf8")).toBe("signature");
    expect(readFileSync(join(out, "Worktree-Cleaner_0.1.0_macos_universal.dmg"), "utf8")).toBe("image");
  });

  it("writes a manifest the updater can read", () => {
    const dir = scratch();
    writeFileSync(join(dir, "Worktree-Cleaner_0.1.0_macos_universal.app.tar.gz"), "archive");
    writeFileSync(join(dir, "Worktree-Cleaner_0.1.0_macos_universal.app.tar.gz.sig"), "the-signature");
    writeFileSync(join(dir, "Worktree-Cleaner_0.1.0_windows_x86_64-setup.exe"), "installer");
    writeFileSync(join(dir, "Worktree-Cleaner_0.1.0_windows_x86_64-setup.exe.sig"), "another-signature");

    const manifest = buildManifest({
      dir,
      version: "0.1.0",
      tag: "v0.1.0",
      repo: "GetSayAll/worktree-cleaner"
    });

    expect(manifest.version).toBe("0.1.0");
    expect(Object.keys(manifest.platforms).sort()).toEqual([
      "darwin-aarch64",
      "darwin-x86_64",
      "windows-x86_64"
    ]);
    expect(manifest.platforms["darwin-aarch64"].signature).toBe("the-signature");
    expect(manifest.platforms["darwin-aarch64"].url).toBe(
      "https://github.com/GetSayAll/worktree-cleaner/releases/download/v0.1.0/Worktree-Cleaner_0.1.0_macos_universal.app.tar.gz"
    );
    expect(manifest.platforms["windows-x86_64"].signature).toBe("another-signature");
  });

  it("publishes nothing that was not signed", () => {
    const dir = scratch();
    writeFileSync(join(dir, "Worktree-Cleaner_0.1.0_macos_universal.app.tar.gz"), "archive");
    expect(() =>
      buildManifest({ dir, version: "0.1.0", tag: "v0.1.0", repo: "GetSayAll/worktree-cleaner" })
    ).toThrow(/no signed artifacts/);
  });
});

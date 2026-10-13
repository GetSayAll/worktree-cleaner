import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { SNAPSHOT_SCHEMA_VERSION } from "./types";

// Rules that are about the shape of the source tree rather than the behaviour of
// one function. Both exist because a plausible future edit would break them
// silently: nothing in the type system or the test suite notices.

const srcDir = fileURLToPath(new URL("./", import.meta.url));

function listSourceFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...listSourceFiles(full));
    else if (/\.(ts|vue)$/.test(entry.name)) files.push(full);
  }
  return files;
}

/**
 * `assess` re-derives a safety level from a reason list. The scanner owns that
 * rule (`from_reasons` in crates/worktree-core/src/reason.rs); a second
 * implementation in the front end would drift from it. The helper therefore
 * exists only to build the mock snapshot, and only tests may import it besides.
 */
const ASSESS_IMPORT = /import\s*\{[^}]*\bassess\b[^}]*\}\s*from\s*["'][^"']*types["']/;

function isAllowedToUseAssess(file: string): boolean {
  const name = file.slice(srcDir.length);
  if (name === "types.ts") return true; // the definition itself
  if (name === "mock/snapshot.ts") return true; // the mock snapshot builder
  return name.endsWith(".test.ts");
}

describe("source tree rules", () => {
  it("keeps the safety-level derivation out of production code", () => {
    const offenders = listSourceFiles(srcDir)
      .filter((file) => !isAllowedToUseAssess(file))
      .filter((file) => ASSESS_IMPORT.test(readFileSync(file, "utf8")))
      .map((file) => file.slice(srcDir.length));
    expect(offenders).toEqual([]);
  });

  it("agrees with the Rust core on the snapshot schema version", () => {
    // The constant is defined twice on purpose — Rust owns the format, and
    // TypeScript owns the browser preview's sample snapshot — and a consumer that
    // reads a version it does not understand must reject the snapshot rather than
    // guess (D20).
    const rust = readFileSync(
      fileURLToPath(new URL("../crates/worktree-core/src/lib.rs", import.meta.url)),
      "utf8"
    );
    const found = rust.match(/pub const SNAPSHOT_SCHEMA_VERSION: u32 = (\d+);/);
    expect(found, "SNAPSHOT_SCHEMA_VERSION not found in the Rust core").not.toBeNull();
    expect(Number(found?.[1])).toBe(SNAPSHOT_SCHEMA_VERSION);
  });
});

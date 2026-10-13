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

// The contract says `src/types.ts` matches the Rust structs field for field, and that
// a change on either side lands in the same commit (DEV_CONTRACT section 3). Nothing
// enforced it: the two could drift and only a runtime failure in the desktop shell
// would say so. The schema version above is the same mechanism applied to one number.
//
// A regex over Rust source is a weaker parse than the compiler — a field written in a
// shape the pattern does not recognise would be missed. That is the mechanism this
// file already uses, and its failure mode is a false pass rather than a false alarm,
// which is why the field counts are also checked against a floor.

const coreDir = fileURLToPath(new URL("../crates/worktree-core/src/", import.meta.url));

function rustFields(source: string, struct: string): string[] | null {
  const block = source.match(new RegExp(`pub struct ${struct}\\s*\\{([\\s\\S]*?)\\n\\}`));
  if (!block) return null;
  return [...block[1].matchAll(/^[ \t]*pub (\w+):/gm)].map((match) => match[1]);
}

function tsFields(source: string, interfaceName: string): string[] | null {
  const block = source.match(
    new RegExp(`export interface ${interfaceName}\\s*\\{([\\s\\S]*?)\\n\\}`)
  );
  if (!block) return null;
  return [...block[1].matchAll(/^ {2}(\w+)\??:/gm)].map((match) => match[1]);
}

describe("the Rust-to-TypeScript field contract", () => {
  const types = readFileSync(join(srcDir, "types.ts"), "utf8");
  const core = {
    "lib.rs": readFileSync(join(coreDir, "lib.rs"), "utf8"),
    "cleanup.rs": readFileSync(join(coreDir, "cleanup.rs"), "utf8"),
    "reason.rs": readFileSync(join(coreDir, "reason.rs"), "utf8")
  };

  const structs: Array<[string, keyof typeof core]> = [
    ["ProjectSnapshot", "lib.rs"],
    ["WorktreeRecord", "lib.rs"],
    ["ArtifactRecord", "lib.rs"],
    ["CleanupPlan", "lib.rs"],
    ["ResultEntry", "lib.rs"],
    ["HistoryEntry", "lib.rs"],
    ["ScanOptions", "lib.rs"],
    ["ExecutionOutcome", "cleanup.rs"],
    ["Reason", "reason.rs"],
    ["SafetyAssessment", "reason.rs"]
  ];

  for (const [name, file] of structs) {
    it(`keeps ${name} field for field`, () => {
      const rust = rustFields(core[file], name);
      const ts = tsFields(types, name);
      expect(rust, `${name} not found in ${file}`).not.toBeNull();
      expect(ts, `${name} not found in src/types.ts`).not.toBeNull();
      // A pattern that stopped matching would otherwise compare two empty lists and
      // pass; every contract struct has at least two fields.
      expect(rust!.length, `${name} parsed as empty`).toBeGreaterThan(1);
      expect(ts!.length, `${name} parsed as empty`).toBeGreaterThan(1);
      expect([...ts!].sort()).toEqual([...rust!].sort());
    });
  }

  it("keeps the reason-code list in step", () => {
    // Rust writes each code twice — once in `ALL`, once in `as_str` — and the string
    // form is the one the front end sees, so it is the one to compare.
    const rustCodes = [...core["reason.rs"].matchAll(/=> "((?:wt|af)\.[a-z_]+)"/g)].map(
      (match) => match[1]
    );
    const block = types.match(/export type ReasonCode =([\s\S]*?);/);
    expect(block, "ReasonCode not found in src/types.ts").not.toBeNull();
    const tsCodes = [...block![1].matchAll(/"([a-z_]+\.[a-z_]+)"/g)].map((match) => match[1]);

    expect(rustCodes.length).toBeGreaterThan(20);
    expect([...tsCodes].sort()).toEqual([...rustCodes].sort());
  });
});

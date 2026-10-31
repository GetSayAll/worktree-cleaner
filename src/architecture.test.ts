import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { SNAPSHOT_SCHEMA_VERSION } from "./types";
import { hygieneRuleKey } from "./format";
import { en } from "./i18n/en";
import { zhCN } from "./i18n/zh-CN";

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
    "reason.rs": readFileSync(join(coreDir, "reason.rs"), "utf8"),
    "hygiene.rs": readFileSync(join(coreDir, "hygiene.rs"), "utf8")
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
    ["SafetyAssessment", "reason.rs"],
    ["ActivityEvent", "lib.rs"],
    ["HygieneFinding", "hygiene.rs"],
    ["HygieneRuleStatus", "hygiene.rs"],
    ["HygieneReport", "hygiene.rs"]
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

  it("gives every reason code a message in every locale", () => {
    // A code with no entry falls back to the core's English sentence, and the
    // fallback is silent — nothing fails, the reader just gets English on a
    // Chinese screen. So the check is on the table's keys, in both locales.
    const rustCodes = [...core["reason.rs"].matchAll(/=> "((?:wt|af)\.[a-z_]+)"/g)].map(
      (match) => match[1]
    );
    const tableCodes = (table: typeof en): string[] =>
      Object.entries(table.reasons).flatMap(([family, entries]) =>
        Object.keys(entries).map((name) => `${family}.${name}`)
      );

    expect(tableCodes(en).sort()).toEqual([...rustCodes].sort());
    expect(tableCodes(zhCN).sort()).toEqual([...rustCodes].sort());
  });

  it("keeps the detail-code list, its type and its copy in step", () => {
    // Three places have to agree, or the result page shows something that means
    // nothing: the `#[serde(rename)]` the core sends, the union `src/types.ts`
    // types it as, and the key `detail.codes` looks it up by. A fourth place is
    // the field names `detail.fields` knows, which the `changed` codes quote.
    const lib = readFileSync(join(coreDir, "lib.rs"), "utf8");
    const rustCodes = [
      ...lib.matchAll(/#\[serde\(rename = "((?:worktree|artifact|prunable|precondition|bundled)[a-z_.]*)"\)\]/g)
    ].map((match) => match[1]);

    const block = types.match(/export type DetailCode =([\s\S]*?);/);
    expect(block, "DetailCode not found in src/types.ts").not.toBeNull();
    const tsCodes = [...block![1].matchAll(/"([a-z_.]+)"/g)].map((match) => match[1]);

    expect(rustCodes.length).toBeGreaterThan(10);
    const tableCodes = (table: typeof en): string[] =>
      Object.entries(table.detail.codes).flatMap(([family, entries]) =>
        Object.keys(entries).map((name) => `${family}.${name}`)
      );

    expect([...tsCodes].sort()).toEqual([...rustCodes].sort());
    expect(tableCodes(en).sort()).toEqual([...rustCodes].sort());
    expect(tableCodes(zhCN).sort()).toEqual([...rustCodes].sort());

    // Every field name the core can send has to be one the sentence can say.
    const fields = ["level", "prunable", "dirty", "untracked", "merged", "size", "files", "tracked", "ignored", "symlink"];
    expect(Object.keys(en.detail.fields).sort()).toEqual([...fields].sort());
    expect(Object.keys(zhCN.detail.fields).sort()).toEqual([...fields].sort());
  });

  it("keeps the activity-action list, its type and its copy in step", () => {
    // The same three places as the detail codes: what the core sends, what the
    // front end types it as, and what the copy table is read by.
    const lib = readFileSync(join(coreDir, "lib.rs"), "utf8");
    const rustActions = [
      ...lib.matchAll(/#\[serde\(rename = "((?:scan|project|plan|settings|cleanup|trash)\.[a-z]+)"\)\]/g)
    ].map((match) => match[1]);

    const block = types.match(/export type ActivityAction =([\s\S]*?);/);
    expect(block, "ActivityAction not found in src/types.ts").not.toBeNull();
    const tsActions = [...block![1].matchAll(/"([a-z]+\.[a-z]+)"/g)].map((match) => match[1]);

    const tableCodes = (table: typeof en): string[] =>
      Object.entries(table.activity.codes).flatMap(([family, entries]) =>
        Object.keys(entries).map((name) => `${family}.${name}`)
      );

    expect(rustActions.length).toBeGreaterThan(5);
    expect([...tsActions].sort()).toEqual([...rustActions].sort());
    expect(tableCodes(en).sort()).toEqual([...rustActions].sort());
    expect(tableCodes(zhCN).sort()).toEqual([...rustActions].sort());
  });

  it("keeps the hygiene rule list, its type and its copy in step", () => {
    // Three places, as above: the identifier the core sends, the union the front
    // end types it as, and the two tables that give it a title and a fix.
    const rustRules = [...core["hygiene.rs"].matchAll(/=> "([A-Z][0-9.]*)"/g)].map(
      (match) => match[1]
    );

    const block = types.match(/export type HygieneRule =([\s\S]*?);/);
    expect(block, "HygieneRule not found in src/types.ts").not.toBeNull();
    const tsRules = [...block![1].matchAll(/"([A-Z][0-9.]*)"/g)].map((match) => match[1]);

    expect(rustRules.length).toBeGreaterThan(20);
    expect([...tsRules].sort()).toEqual([...rustRules].sort());
    for (const table of [en, zhCN]) {
      // `B1.1` reaches the UI with its dot; the table spells it `B11` because a
      // key may not carry one (see `hygieneRuleKey`).
      expect(Object.keys(table.hygiene.rules).sort()).toEqual(rustRules.map(hygieneRuleKey).sort());
      expect(Object.keys(table.hygiene.fixes).sort()).toEqual(rustRules.map(hygieneRuleKey).sort());
    }
  });

  it("gives every hygiene situation a sentence under the rule that raises it", () => {
    // A finding with no entry falls back to its evidence line: true, but it leaves
    // the reader without the one thing the table is for. The states sit under the
    // rule they belong to, so the two have to agree twice over — the rule has to
    // exist and the situation has to be filed under it.
    const source = core["hygiene.rs"];

    // Three findings choose their situation in a `let` before they are built, so
    // a scan of `.state("…")` alone would miss them; those definitions are read
    // too, and attributed to the finding that follows them.
    const assigned = [...source.matchAll(/let state = [^;]*;/g)].map((match) => ({
      at: match.index ?? 0,
      states: [...match[0].matchAll(/"([a-z0-9_]+)"/g)].map((inner) => inner[1])
    }));

    const rustPairs: string[] = [];
    for (const call of source.matchAll(/HygieneFinding::new\(\s*HygieneRule::([A-Z][0-9.]*)/g)) {
      const rule = hygieneRuleKey(call[1]);
      const from = (call.index ?? 0) + call[0].length;
      const stateCall = /\.state\(([^)]*)\)/.exec(source.slice(from, from + 900));
      if (!stateCall) continue;
      const argument = stateCall[1].trim();
      if (argument.startsWith('"')) {
        rustPairs.push(`${rule}.${argument.replace(/"/g, "")}`);
        continue;
      }
      const at = from + stateCall.index;
      const definition = assigned.filter((entry) => entry.at < at).pop();
      for (const situation of definition?.states ?? []) rustPairs.push(`${rule}.${situation}`);
    }
    // The unobservable rules are built from a table rather than one call each, so
    // their pairing is read from that table.
    const unobservable = /const NOT_CHECKABLE[\s\S]*?\n\];/.exec(source)?.[0] ?? "";
    for (const rule of unobservable.matchAll(/HygieneRule::([A-Z][0-9.]*)/g)) {
      rustPairs.push(`${hygieneRuleKey(rule[1])}.not_checkable`);
    }

    const rustStates = [
      ...new Set([
        ...[...source.matchAll(/\.state\("([a-z0-9_]+)"\)/g)].map((match) => match[1]),
        ...assigned.flatMap((entry) => entry.states)
      ])
    ];
    const pairs = (table: typeof en): string[] =>
      Object.entries(table.hygiene.states).flatMap(([rule, entries]) =>
        Object.keys(entries).map((situation) => `${rule}.${situation}`)
      );

    expect(rustPairs.length).toBeGreaterThan(30);
    expect(rustStates.length).toBeGreaterThan(30);
    for (const table of [en, zhCN]) {
      // The core's sentence is the one that must exist. A situation filed under
      // the wrong rule — the plausible mistake, since the names are shared across
      // families — shows up here and nowhere else.
      const missing = rustPairs.filter((pair) => !pairs(table).includes(pair));
      expect(missing.sort()).toEqual([]);
      expect([...new Set(pairs(table).map((pair) => pair.slice(pair.indexOf(".") + 1)))]
        .sort())
        .toEqual([...rustStates].sort());
    }

    // Every rule key is one this build knows: a typo would otherwise leave a
    // situation with two sentences and its real one with none, and the pair
    // comparison above could not tell.
    const block = types.match(/export type HygieneRule =([\s\S]*?);/);
    const rules = [...(block?.[1] ?? "").matchAll(/"([A-Z][0-9.]*)"/g)].map((match) =>
      hygieneRuleKey(match[1])
    );
    expect(rules.length).toBeGreaterThan(20);
    for (const table of [en, zhCN]) {
      for (const rule of Object.keys(table.hygiene.states)) {
        expect(rules.includes(rule), `${rule} is not a known rule id`).toBe(true);
      }
    }
  });
});

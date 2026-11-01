import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * The type scale is eight tokens in `:root`, and no other size exists.
 *
 * Why this file exists: the stylesheet had accumulated thirteen sizes - 10, 10.5,
 * 11, 11.5, 12, 12.5, 13, 13.5, 14, 15, 22, 26, 30 - and five of them sat inside
 * three pixels of each other. A scale like that is not a scale: it is a record of
 * each decision made on its own, and it is why two rows that look alike were set
 * one point apart. Nothing failed when a fifth step was added, so a fifth step
 * kept being added.
 *
 * The rules below are the ones a stylesheet cannot state about itself:
 *
 *   - eight steps, no more;
 *   - never below 12, the size at which the Chinese fallback still holds;
 *   - ascending, because a scale read out of order is a list;
 *   - every step used, so no step survives as decoration;
 *   - no `font-size` anywhere else, so a ninth step cannot appear by accident.
 *
 * Reading the source rather than the rendered page is deliberate: this is a rule
 * about what the stylesheet is allowed to say, and a browser would only report
 * the result.
 */

const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const withoutComments = css.replace(/\/\*[\s\S]*?\*\//g, " ");

/** The body of the first `:root {` block, braces balanced. */
function rootBody(source: string): string {
  const start = source.indexOf(":root {");
  if (start < 0) throw new Error("no :root block found");
  const open = source.indexOf("{", start);
  let depth = 1;
  for (let index = open + 1; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    else if (source[index] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, index);
    }
  }
  throw new Error("unbalanced braces after :root");
}

const root = rootBody(withoutComments);
const outside = withoutComments.slice(0, withoutComments.indexOf(":root {")) +
  withoutComments.slice(withoutComments.indexOf(":root {") + root.length + ":root {".length + 1);

const scale = [...root.matchAll(/(--fs-[a-z]+):\s*([0-9.]+)px/g)].map((match) => ({
  name: match[1],
  size: Number(match[2])
}));

describe("the type scale", () => {
  it("has exactly eight steps", () => {
    expect(scale.map((step) => step.name)).toEqual([
      "--fs-caption",
      "--fs-ui",
      "--fs-body",
      "--fs-title",
      "--fs-lead",
      "--fs-page",
      "--fs-metric",
      "--fs-hero"
    ]);
  });

  it("starts at 12px and never goes below it", () => {
    // The floor is a legibility decision, not a round number: the Chinese UI text
    // falls back to PingFang / Microsoft YaHei, and below 12 those fallbacks lose
    // the stroke contrast that makes them readable at this density.
    const smallest = Math.min(...scale.map((step) => step.size));
    expect(smallest).toBe(12);
  });

  it("is ascending, with no two steps the same", () => {
    const sizes = scale.map((step) => step.size);
    expect(sizes).toEqual([...sizes].sort((a, b) => a - b));
    expect(new Set(sizes).size).toBe(sizes.length);
  });

  it("uses every step", () => {
    // A step nothing references is a step that survived a redesign. Either give it
    // a job or delete it - the next person will otherwise assume it means something.
    for (const step of scale) {
      const uses = outside.match(new RegExp(`var\\(${step.name}\\)`, "g")) ?? [];
      expect(uses.length, `${step.name} is declared but never used`).toBeGreaterThan(0);
    }
  });

  it("leaves no font size outside the scale", () => {
    // This is the guard that keeps the count at eight. A literal size here is
    // either a typo or a ninth step arriving without a decision.
    const written = [...outside.matchAll(/font-size:\s*([^;}]+)/g)].map((match) => match[1].trim());
    expect(written.length).toBeGreaterThan(20);
    const strays = written.filter((value) => !/^var\(--fs-[a-z]+\)$/.test(value));
    expect(strays).toEqual([]);
  });
});

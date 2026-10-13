import { readFileSync } from "node:fs";
import { nextTick } from "vue";
import { describe, expect, it, vi } from "vitest";

/**
 * The dark palette exists twice on purpose: once behind `prefers-color-scheme`
 * (the "System" choice) and once behind the `[data-theme]` attribute (the manual
 * choice). Plain CSS cannot share a block between a media query and a selector,
 * so these tests are the guard that keeps the two copies from drifting apart.
 */

interface Block {
  selector: string;
  declarations: Record<string, string>;
}

/**
 * Flattens `selector { declarations }` pairs; nested blocks keep their outer text.
 *
 * Comments are stripped first. Without that, a comment sitting between a
 * declaration and the next one is read as the start of that declaration's name
 * and the token silently vanishes from the parsed set — which is how a comment
 * on `--sp-7` took `--shadow` out of this test's view while the stylesheet itself
 * stayed perfectly valid. A comment before a selector leaked into the selector
 * text for the same reason. Both failures were invisible in the browser.
 */
function parseBlocks(source: string): Block[] {
  const withoutComments = source.replace(/\/\*[\s\S]*?\*\//g, " ");
  const blocks: Block[] = [];
  for (const match of withoutComments.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const declarations: Record<string, string> = {};
    for (const entry of match[2].split(";")) {
      const colon = entry.indexOf(":");
      if (colon < 0) continue;
      const name = entry.slice(0, colon).trim();
      const value = entry.slice(colon + 1).trim();
      if (name) declarations[name] = value;
    }
    blocks.push({ selector: match[1].trim(), declarations });
  }
  return blocks;
}

const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const blocks = parseBlocks(css);

/**
 * Body of a top-level at-rule block, braces balanced. `parseBlocks` flattens
 * nested blocks away, which loses the context a `@media` query provides; these
 * tests need that context back.
 */
function atRuleBody(prelude: string): string {
  const start = css.indexOf(prelude);
  if (start < 0) throw new Error(`no at-rule block found for ${prelude}`);
  const open = css.indexOf("{", start);
  if (open < 0) throw new Error(`malformed at-rule block for ${prelude}`);
  let depth = 1;
  for (let index = open + 1; index < css.length; index += 1) {
    if (css[index] === "{") depth += 1;
    else if (css[index] === "}") {
      depth -= 1;
      if (depth === 0) return css.slice(open + 1, index);
    }
  }
  throw new Error(`unbalanced braces after ${prelude}`);
}

function block(predicate: (selector: string) => boolean, label: string): Block {
  const found = blocks.find((entry) => predicate(entry.selector));
  if (!found) throw new Error(`no CSS block found for ${label}`);
  return found;
}
const light = block((selector) => selector === ":root", "the light :root block");
const mediaDark = block(
  (selector) => selector.includes(':root:not([data-theme="light"])'),
  "the prefers-color-scheme dark block"
);
const attributeDark = block((selector) => selector === ':root[data-theme="dark"]', "the [data-theme=dark] block");

const tokens = (entry: Block) =>
  Object.fromEntries(Object.entries(entry.declarations).filter(([name]) => name.startsWith("--")));

describe("theme stylesheet", () => {
  it("declares the same palette in both dark blocks", () => {
    // Same names, same values: the manual choice and the OS preference must
    // produce an identical dark theme.
    expect(tokens(attributeDark)).toEqual(tokens(mediaDark));
  });

  it("only overrides tokens that already have a light default", () => {
    // A token defined only in dark would resolve to nothing in light mode.
    for (const name of Object.keys(tokens(mediaDark))) {
      expect(light.declarations, `${name} has no light default`).toHaveProperty(name);
    }
  });

  it("keeps the radius and font tokens out of the dark overrides", () => {
    // Layout tokens are not theme dependent; overriding them would be a mistake.
    for (const name of ["--radius-sm", "--radius", "--radius-lg", "--mono"]) {
      expect(mediaDark.declarations).not.toHaveProperty(name);
      expect(attributeDark.declarations).not.toHaveProperty(name);
    }
  });

  it("flips color-scheme in both directions", () => {
    expect(light.declarations["color-scheme"]).toBe("light");
    expect(mediaDark.declarations["color-scheme"]).toBe("dark");
    expect(attributeDark.declarations["color-scheme"]).toBe("dark");
  });

  it("uses a genuinely different dark canvas and ink", () => {
    // Presence first: `not.toBe` on two missing declarations would pass while
    // proving nothing.
    expect(mediaDark.declarations).toHaveProperty("--bg");
    expect(light.declarations).toHaveProperty("--bg");
    expect(mediaDark.declarations).toHaveProperty("--text");
    expect(light.declarations).toHaveProperty("--text");
    // Guards against a "dark" palette that is really the light one.
    expect(mediaDark.declarations["--bg"]).not.toBe(light.declarations["--bg"]);
    expect(mediaDark.declarations["--text"]).not.toBe(light.declarations["--text"]);
  });

  it("keeps the spacing scale out of the dark overrides", () => {
    // The scale is layout, not palette: overriding it per theme would make two
    // dark modes space a page differently from each other.
    for (const block of [mediaDark, attributeDark]) {
      for (const name of Object.keys(light.declarations).filter((n) => n.startsWith("--sp-"))) {
        expect(block.declarations, `${name} must not be a theme token`).not.toHaveProperty(name);
      }
    }
  });

  it("reads every token the stylesheet declares", () => {
    // A guard on the guard. `parseBlocks` had no notion of comments, so one
    // sitting between two declarations was read as the start of the next
    // declaration's name and the token vanished from the parsed set — `--shadow`
    // went missing while a spacing scale was being added, and the only symptom
    // was a failure far from the cause. It strips comments now; this asserts the
    // strip holds in both directions.
    //
    // Read from the source text rather than from `parseBlocks`, so the two sides
    // are genuinely independent. Only the light block: the dark ones are checked
    // against it by the tests above. A scan of the whole file would pick up
    // `--primary` in `.btn--primary`, which is a class modifier, not a token.
    const body = atRuleBody(":root {");
    const written = [...body.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((match) => match[1]);
    expect(written.length, "the light block declares no tokens at all?").toBeGreaterThan(0);
    for (const name of new Set(written)) {
      expect(light.declarations, `${name} is declared but unreadable`).toHaveProperty(name);
    }

    // And nothing parsed carries a comment or a fragment in its name. Ordinary
    // properties (`color-scheme`) live alongside the tokens and are left alone.
    for (const name of Object.keys(light.declarations).filter((n) => n.startsWith("-"))) {
      expect(name, "a parsed token name must be a bare identifier").toMatch(/^--[a-z0-9-]+$/);
    }
  });

  it("keeps the OS-preference palette inside its media query", () => {
    // The selector text alone proves nothing: the same text outside
    // `prefers-color-scheme: dark` would paint dark in light mode too.
    const mediaBody = atRuleBody("@media (prefers-color-scheme: dark)");
    expect(mediaBody).toContain(':root:not([data-theme="light"])');
    // The manual pin is the sibling of that query, never a member of it.
    expect(mediaBody).not.toContain(':root[data-theme="dark"]');
  });
});

/** Minimal stand-in for an element: `dataset.foo` really maps to `data-foo`. */
function fakeElement() {
  const attributes = new Map<string, string>();
  const dataset = new Proxy(
    {},
    {
      get: (_target, prop: string) => attributes.get(`data-${prop}`),
      set: (_target, prop: string, value) => {
        attributes.set(`data-${prop}`, String(value));
        return true;
      },
      has: (_target, prop: string) => attributes.has(`data-${prop}`)
    }
  );
  return {
    dataset,
    getAttribute: (name: string) => attributes.get(name) ?? null,
    setAttribute: (name: string, value: string) => attributes.set(name, value),
    removeAttribute: (name: string) => attributes.delete(name)
  };
}

describe("theme application", () => {
  it("writes and clears data-theme on the document element", async () => {
    const root = fakeElement();
    vi.stubGlobal("document", { documentElement: root });

    try {
      // Imported after the stub exists so the watcher's immediate run sees it.
      vi.resetModules();
      const { settings } = await import("./state");

      // The default is "system", which must not leave an attribute behind.
      expect(root.getAttribute("data-theme")).toBeNull();

      settings.theme = "dark";
      await nextTick();
      expect(root.getAttribute("data-theme")).toBe("dark");

      settings.theme = "light";
      await nextTick();
      expect(root.getAttribute("data-theme")).toBe("light");

      // And back the other way: the pin is one attribute, so switching must
      // overwrite it, never stack a second value or leave the old one behind.
      settings.theme = "dark";
      await nextTick();
      expect(root.getAttribute("data-theme")).toBe("dark");

      settings.theme = "system";
      await nextTick();
      expect(root.getAttribute("data-theme")).toBeNull();
    } finally {
      vi.unstubAllGlobals();
      vi.resetModules();
    }
  });
});

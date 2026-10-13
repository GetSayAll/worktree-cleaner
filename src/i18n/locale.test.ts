import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { en } from "./en";
import { zhCN } from "./zh-CN";
import {
  applyLocale,
  isLocale,
  isLocaleChoice,
  localeChoice,
  LOCALES,
  messages,
  resolveLocale,
  SOURCE_LOCALE,
  systemLocale,
  watchSystemLocale
} from "./index";

// The tables are the one place D23 allows non-English text, which makes them the
// one place where a typo cannot be caught by reading the diff: a missing key
// would silently fall back to whatever the string was before. These tests are
// what keeps a translation honest.

/** Every leaf path of a message table, e.g. `nav.project`. */
function leafPaths(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) return [prefix];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    leafPaths(child, prefix ? `${prefix}.${key}` : key)
  );
}

/** The leaves as `path -> typeof`, so a string that became a function shows up. */
function leafShapes(value: unknown, prefix = ""): Record<string, string> {
  const walk = (node: unknown, path: string, out: Record<string, string>) => {
    if (typeof node !== "object" || node === null) {
      out[path] = typeof node;
      return out;
    }
    for (const [key, child] of Object.entries(node as Record<string, unknown>)) {
      walk(child, path ? `${path}.${key}` : key, out);
    }
    return out;
  };
  return walk(value, prefix, {});
}

function arityOf(value: unknown, path: string): number | null {
  const parts = path.split(".");
  let node: unknown = value;
  for (const part of parts) {
    if (typeof node !== "object" || node === null) return null;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "function" ? (node as (...args: unknown[]) => unknown).length : null;
}

describe("message tables", () => {
  beforeEach(() => applyLocale(SOURCE_LOCALE));
  afterEach(() => vi.unstubAllGlobals());

  it("ships every locale as a table with the same shape as the source", () => {
    const sourcePaths = leafPaths(en).sort();
    expect(sourcePaths.length).toBeGreaterThan(100);

    for (const locale of LOCALES) {
      const table = locale === "en" ? en : zhCN;
      const paths = leafPaths(table).sort();
      expect(paths, `${locale} has different keys than en.ts`).toEqual(sourcePaths);

      const sourceShapes = leafShapes(en);
      for (const [path, kind] of Object.entries(leafShapes(table))) {
        expect(kind, `${locale}.${path} changed kind`).toBe(sourceShapes[path]);
        if (kind === "function") {
          expect(
            arityOf(table, path),
            `${locale}.${path} takes a different number of arguments`
          ).toBe(arityOf(en, path));
        }
      }
    }
  });

  it("leaves no translation empty", () => {
    for (const locale of LOCALES) {
      const table = locale === "en" ? en : zhCN;
      for (const path of leafPaths(table)) {
        const parts = path.split(".");
        let node: unknown = table;
        for (const part of parts) node = (node as Record<string, unknown>)[part];
        if (typeof node !== "string") continue;
        // `enDash`/`units.atLeast` are punctuation and spaces on purpose.
        expect(node.length, `${locale}.${path} is empty`).toBeGreaterThan(0);
      }
    }
  });
});

describe("system locale detection", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("maps any Chinese tag onto the Chinese table", () => {
    for (const tag of ["zh", "zh-CN", "zh-Hans-CN", "zh-TW", "zh-Hant-HK"]) {
      vi.stubGlobal("navigator", { languages: [tag] });
      expect(systemLocale(), tag).toBe("zh-CN");
    }
  });

  it("falls back to the source locale for a language it does not ship", () => {
    vi.stubGlobal("navigator", { languages: ["ja-JP", "de"] });
    expect(systemLocale()).toBe(SOURCE_LOCALE);
  });

  it("picks the first shipped language in the preference list", () => {
    vi.stubGlobal("navigator", { languages: ["fr-FR", "zh-CN", "en"] });
    expect(systemLocale()).toBe("zh-CN");

    vi.stubGlobal("navigator", { languages: ["fr-FR", "en-GB", "zh-CN"] });
    expect(systemLocale()).toBe("en");
  });

  it("reads the single-language fallback when there is no preference list", () => {
    vi.stubGlobal("navigator", { language: "zh-Hans" });
    expect(systemLocale()).toBe("zh-CN");
  });
});

describe("applyLocale", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("rebinds every section, not just the one the caller knew about", () => {
    applyLocale("en");
    const english = messages.nav.project;
    expect(english).toBe(en.nav.project);

    applyLocale("zh-CN");
    expect(messages.nav.project).toBe(zhCN.nav.project);
    expect(messages.nav.project).not.toBe(english);
    // A section a caller never touches is rebound too.
    expect(messages.settings.locale.system).toBe(zhCN.settings.locale.system);
    expect(messages.status.label.Safe).toBe(zhCN.status.label.Safe);
  });

  it("follows the system when asked to, and records the choice", () => {
    vi.stubGlobal("navigator", { languages: ["zh-CN"] });
    expect(applyLocale("system")).toBe("zh-CN");
    expect(localeChoice()).toBe("system");
    expect(messages.nav.project).toBe(zhCN.nav.project);

    expect(resolveLocale("en")).toBe("en");
    applyLocale("en");
    expect(localeChoice()).toBe("en");
  });

  it("sets the document language", () => {
    const documentElement = { lang: "" };
    vi.stubGlobal("document", { documentElement });

    applyLocale("zh-CN");
    expect(documentElement.lang).toBe("zh-CN");

    applyLocale("en");
    expect(documentElement.lang).toBe("en");
  });

  it("only follows the system while the user has not pinned a language", () => {
    vi.stubGlobal("navigator", { languages: ["en"] });
    const listeners: Array<() => void> = [];
    vi.stubGlobal("window", {
      addEventListener: (type: string, handler: () => void) => {
        if (type === "languagechange") listeners.push(handler);
      },
      removeEventListener: () => {}
    });
    const seen: string[] = [];
    const teardown = watchSystemLocale((locale) => seen.push(locale));

    applyLocale("system");
    vi.stubGlobal("navigator", { languages: ["zh-CN"] });
    listeners.forEach((handler) => handler());
    expect(seen).toEqual(["zh-CN"]);

    applyLocale("en");
    listeners.forEach((handler) => handler());
    expect(seen, "a pinned language is not overridden by the system").toEqual(["zh-CN"]);

    teardown();
  });
});

describe("stored values", () => {
  it("accepts only what the app ships", () => {
    expect(isLocale("en")).toBe(true);
    expect(isLocale("zh-CN")).toBe(true);
    expect(isLocale("zh")).toBe(false);
    expect(isLocale("fr")).toBe(false);
    expect(isLocale(undefined)).toBe(false);

    expect(isLocaleChoice("system")).toBe(true);
    expect(isLocaleChoice("zh-CN")).toBe(true);
    expect(isLocaleChoice("auto")).toBe(false);
    expect(isLocaleChoice(null)).toBe(false);
  });
});

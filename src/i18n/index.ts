// Single swap point for the UI language.
//
// Everything a user can read goes through `messages`, and switching a language
// is one assignment into it. That works only because `messages` is reactive:
// a component that reads `messages.nav.project` inside its template re-renders,
// while anything that copied a string at module load (a label map, a constant
// array, a helper's return value) keeps the old language forever.
//
// So the rule for the rest of the codebase is: **read copy at render time, not
// at import time.** Where a value has to be computed, compute it in a `computed`
// or a function that runs on each render, never in a module-level constant.
//
// Adding a locale: import its table, add the tag to `LOCALES` and to `TABLES`.
// `locale.test.ts` asserts that every table has exactly the same keys as
// `en.ts`, so a missing translation fails the build instead of showing English
// inside a Chinese page.

import { reactive } from "vue";
import { en } from "./en";
import { zhCN } from "./zh-CN";
import { hostOs, type HostOs } from "../platform";

/** The message table's shape. `en` is the source of truth for it. */
export type Messages = typeof en;

/** The locales the app ships. The values are BCP 47 tags. */
export const LOCALES = ["en", "zh-CN"] as const;

export type Locale = (typeof LOCALES)[number];

/** What the user picks in Settings: one language, or follow the system. */
export type LocaleChoice = "system" | Locale;

/** The source locale: also the fallback when the system language is neither. */
export const SOURCE_LOCALE: Locale = "en";

const TABLES: Record<Locale, Messages> = { en, "zh-CN": zhCN };

/**
 * The wording this machine needs, filled into the `{{token}}` holes.
 *
 * A table writes `{{trash}}` and the resolved table carries the word that is true
 * here, so no string has to name two systems to be correct and no call site has to
 * know what it is running on. The tokens are resolved when a locale is applied
 * rather than where they are read: the host does not change while the app runs, and
 * `messages` is a plain string table by the time any component sees it.
 */
function tokensFor(locale: Locale, os: HostOs): Record<string, string> {
  return { trash: TABLES[locale].platform.trash[os] };
}

/** Every `{{token}}` in `value`, replaced. A string with no token is returned as it is. */
function fill(value: string, tokens: Record<string, string>): string {
  return value.replace(/\{\{(\w+)\}\}/g, (match, name: string) => tokens[name] ?? match);
}

/**
 * The table for one locale on one machine, with its tokens filled in.
 *
 * The tables are small and a locale is applied at most a few times per run, so the
 * simplest thing that cannot alias the source table is the right one: a structural
 * copy, rebuilt rather than mutated.
 *
 * Functions are wrapped rather than copied: most messages are `(n) => ...`, and a
 * token inside one of those bodies is as real as a token in a plain string — the
 * one that was `went to the {{trash}}` in a function body is what would have shown
 * the reader a literal `{{trash}}` if this only walked strings. The wrapper fills
 * the string the function returns, which is the only place a token can appear.
 */
export function resolvedTable(locale: Locale, os: HostOs): Messages {
  const table = TABLES[locale];
  const tokens = tokensFor(locale, os);
  const walk = (node: unknown): unknown => {
    if (typeof node === "string") return fill(node, tokens);
    if (typeof node === "function") {
      const fn = node as (...args: unknown[]) => unknown;
      return (...args: unknown[]) => {
        const value = fn(...args);
        return typeof value === "string" ? fill(value, tokens) : value;
      };
    }
    if (Array.isArray(node)) return node.map(walk);
    if (node && typeof node === "object") {
      const out: Record<string, unknown> = {};
      for (const [key, value] of Object.entries(node)) out[key] = walk(value);
      return out;
    }
    return node;
  };
  return walk(table) as Messages;
}

/**
 * The live message table.
 *
 * Reactive on purpose: components read it in their templates, so replacing a
 * section re-renders them. The tables are two levels deep and all three levels
 * are plain objects, so `Object.assign` replaces whole sections; that is safe
 * only while every table has the same keys, which the locale test asserts.
 *
 * Initialised resolved rather than from the raw table, so the first paint has no
 * `{{trash}}` in it even if nothing has called `applyLocale` yet.
 */
export const messages = reactive<Messages>(resolvedTable(SOURCE_LOCALE, hostOs()));

let choice: LocaleChoice = "system";

/** The language the system asks for, falling back to the source locale. */
export function systemLocale(): Locale {
  const navigatorLanguages =
    typeof navigator === "undefined"
      ? []
      : navigator.languages && navigator.languages.length > 0
        ? navigator.languages
        : [navigator.language];
  for (const tag of navigatorLanguages) {
    const normalised = (tag ?? "").toLowerCase();
    // Only the language subtag is compared: `zh-Hans-CN`, `zh-TW` and `zh` all
    // have to land on the same table while the app ships one Chinese locale.
    if (normalised.startsWith("zh")) return "zh-CN";
    if (normalised.startsWith("en")) return "en";
  }
  return SOURCE_LOCALE;
}

/** What the user currently asked for. */
export function localeChoice(): LocaleChoice {
  return choice;
}

/** Which table `applyLocale` would resolve `next` to. */
export function resolveLocale(next: LocaleChoice): Locale {
  return next === "system" ? systemLocale() : next;
}

/**
 * Switches the language and returns the locale actually in effect.
 *
 * Also updates the document language, which is what screen readers and the
 * browser's own text handling read.
 */
export function applyLocale(next: LocaleChoice): Locale {
  choice = next;
  const resolvedLocale = resolveLocale(next);
  Object.assign(messages, resolvedTable(resolvedLocale, hostOs()));
  if (typeof document !== "undefined") {
    document.documentElement.lang = resolvedLocale;
  }
  return resolvedLocale;
}

/**
 * Follows the system language while the user has not pinned one.
 *
 * `languagechange` fires when the OS language changes; a pinned choice is left
 * alone, because the user asked for it explicitly.
 *
 * Returns a teardown function.
 */
export function watchSystemLocale(onChange?: (locale: Locale) => void): () => void {
  if (typeof window === "undefined") return () => {};
  const handler = () => {
    if (choice !== "system") return;
    const locale = applyLocale("system");
    onChange?.(locale);
  };
  window.addEventListener("languagechange", handler);
  return () => window.removeEventListener("languagechange", handler);
}

/** Whether a stored value is one of the locales the app ships. */
export function isLocale(value: unknown): value is Locale {
  return typeof value === "string" && (LOCALES as readonly string[]).includes(value);
}

/** Whether a stored value is a valid Settings choice. */
export function isLocaleChoice(value: unknown): value is LocaleChoice {
  return value === "system" || isLocale(value);
}

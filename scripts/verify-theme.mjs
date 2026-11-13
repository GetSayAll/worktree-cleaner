#!/usr/bin/env node
// Verify the three appearance modes in a real browser.
//
// Why this exists: the theme is implemented as two independent CSS blocks (the
// `prefers-color-scheme` media query and the `[data-theme]` attribute) plus a
// Vue watcher that writes the attribute. Unit tests can only check the text;
// only a real engine proves the cascade resolves to the palette the user sees.
//
// What it checks, for every combination of OS preference (light/dark) and the
// in-app choice (system/light/dark):
//   1. `data-theme` on <html> is exactly what the choice implies,
//   2. the resolved custom properties match the expected palette,
//   3. the actually painted background of the app shell is that palette.
// A screenshot of each combination is written to dist/theme-shots/.
//
// Usage: node scripts/verify-theme.mjs [path-to-html]   (default dist/preview.html)
// Env:   CHROME_PATH=/path/to/Chrome   (default: the macOS Chrome bundle)

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pagePath = resolve(process.argv[2] ?? join(root, "dist", "preview.html"));
const shotDir = join(root, "dist", "theme-shots");
const CHROME = process.env.CHROME_PATH ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const pageUrl = `file://${pagePath}`;

// A CDP round trip that never answers would hang the whole run: every send is
// bounded, and a socket that closes early fails the pending call loudly instead
// of leaving the script waiting forever.
const SEND_TIMEOUT_MS = 15_000;

// Expected palettes, copied from the two source-of-truth blocks in src/styles.css.
// `canvas`/`surface`/`ink` are the painted rgb() values a browser reports for the
// elements that carry them: body (--bg), .sidebar (--surface) and its text (--text).
const LIGHT = {
  bg: "#f5f6f8",
  surface: "#ffffff",
  text: "#111418",
  canvas: "rgb(245, 246, 248)",
  surfaceRgb: "rgb(255, 255, 255)",
  ink: "rgb(17, 20, 24)"
};
const DARK = {
  bg: "#0e1013",
  surface: "#15181d",
  text: "#e7eaee",
  canvas: "rgb(14, 16, 19)",
  surfaceRgb: "rgb(21, 24, 29)",
  ink: "rgb(231, 234, 238)"
};

/** [OS preference, in-app choice, expected data-theme, expected palette] */
const CASES = [
  ["dark", "system", null, DARK],
  ["dark", "light", "light", LIGHT],
  ["dark", "dark", "dark", DARK],
  ["light", "system", null, LIGHT],
  ["light", "light", "light", LIGHT],
  ["light", "dark", "dark", DARK]
];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

class Cdp {
  #ws;
  #next = 0;
  #pending = new Map();
  sessionId = null;

  /**
   * Console output and uncaught errors seen since the page started.
   *
   * A page that fails to mount leaves no other trace: the DOM is empty, the HTTP
   * status is fine, and "never finished loading" says nothing about why. This is
   * what makes that failure reportable.
   */
  diagnostics = [];

  constructor(ws) {
    this.#ws = ws;
    ws.addEventListener("message", (event) => {
      const msg = JSON.parse(event.data);
      if (msg.id !== undefined) {
        const entry = this.#pending.get(msg.id);
        if (!entry) return;
        this.#pending.delete(msg.id);
        if (msg.error) entry.reject(new Error(`${msg.error.message} (${JSON.stringify(msg.error.data ?? "")})`));
        else entry.resolve(msg.result);
        return;
      }
      this.#collect(msg);
    });
    ws.addEventListener("close", () => {
      // Chrome died mid-run (crash, kill, tab close). Every in-flight call must
      // fail now rather than resolve never.
      for (const entry of this.#pending.values()) {
        entry.reject(new Error("the DevTools socket closed before the call was answered"));
      }
      this.#pending.clear();
    });
  }

  #collect(msg) {
    if (msg.method === "Runtime.consoleAPICalled") {
      const text = msg.params.args
        .map((arg) => arg.value ?? arg.description ?? arg.type)
        .join(" ");
      this.diagnostics.push(`console.${msg.params.type}: ${text}`);
    } else if (msg.method === "Runtime.exceptionThrown") {
      const details = msg.params.exceptionDetails;
      this.diagnostics.push(`uncaught: ${details.text} ${details.exception?.description ?? ""}`);
    } else if (msg.method === "Log.entryAdded" && msg.params.entry.level === "error") {
      this.diagnostics.push(`page error: ${msg.params.entry.text} ${msg.params.entry.url ?? ""}`);
    }
  }

  /** Everything collected so far, or a note that nothing was. */
  report() {
    return this.diagnostics.length > 0 ? `\n${this.diagnostics.join("\n")}` : " (nothing was logged)";
  }

  send(method, params = {}) {
    const id = ++this.#next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error(`${method} timed out after ${SEND_TIMEOUT_MS}ms`));
      }, SEND_TIMEOUT_MS);
      this.#pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        }
      });
      const message = { id, method, params };
      // Omitted rather than null: an empty-string sessionId is rejected by Chrome.
      if (this.sessionId) message.sessionId = this.sessionId;
      this.#ws.send(JSON.stringify(message));
    });
  }

  /** Evaluate an expression, optionally awaiting a promise result. */
  async evaluate(expression, { awaitPromise = false, returnByValue = true } = {}) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise,
      returnByValue
    });
    if (result.exceptionDetails) {
      throw new Error(`page script failed: ${result.exceptionDetails.text} ${result.exceptionDetails.exception?.description ?? ""}`);
    }
    return result.result.value;
  }
}

async function launchChrome(userDataDir) {
  const child = spawn(
    CHROME,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-sandbox",
      "--hide-scrollbars",
      "--window-size=1440,900",
      "--remote-debugging-port=0",
      // A private profile per run: without it Chrome can attach to (or refuse to
      // start because of) a profile locked by the user's normal browser.
      `--user-data-dir=${userDataDir}`,
      "about:blank"
    ],
    { stdio: ["ignore", "ignore", "pipe"] }
  );
  const wsUrl = await new Promise((resolve, reject) => {
    let text = "";
    const timer = setTimeout(() => reject(new Error(`Chrome did not expose DevTools:\n${text.slice(0, 800)}`)), 20000);
    child.stderr.on("data", (chunk) => {
      text += chunk;
      const match = text.match(/DevTools listening on (ws:\/\/\S+)/);
      if (match) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    });
    child.on("exit", (code) => reject(new Error(`Chrome exited early with code ${code}`)));
  });
  return { child, wsUrl };
}

/** SIGTERM, then SIGKILL if the process is still alive after a moment. */
async function stopChrome(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill();
  const deadline = Date.now() + 3000;
  while (Date.now() < deadline && child.exitCode === null && child.signalCode === null) {
    await sleep(50);
  }
  if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
}

/** Wait until the SPA has mounted and is idle for two animation frames. */
async function settle(cdp) {
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    ready = await cdp.evaluate(
      `document.readyState === "complete" && !!document.querySelector(".nav__item")`
    );
    if (ready) break;
    await sleep(100);
  }
  // A page that never mounts would make every later assertion meaningless; fail
  // here with the reason — including whatever the page said on the way down —
  // instead of reporting confusing palette mismatches.
  if (!ready) {
    const state = await cdp.evaluate(
      `JSON.stringify({ ready: document.readyState, navItems: document.querySelectorAll(".nav__item").length, appLength: (document.getElementById("app")?.innerHTML ?? "").length })`
    );
    throw new Error(`the page never finished loading (.nav__item never appeared): ${state}${cdp.report()}`);
  }
  await cdp.evaluate(
    `new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done(null))))`,
    { awaitPromise: true }
  );
}

/**
 * Click the one element matching `selector`.
 *
 * By selector rather than by label: the UI follows the system language, so a
 * button's text depends on the machine the check runs on, and "Settings" is not
 * "设置".
 */
async function clickButton(cdp, selector) {
  const clicked = await cdp.evaluate(`
    (() => {
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return "no element matches " + ${JSON.stringify(selector)};
      el.click();
      return null;
    })()
  `);
  if (clicked) throw new Error(clicked);
  await cdp.evaluate(
    `new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done(null))))`,
    { awaitPromise: true }
  );
}

async function readState(cdp) {
  return cdp.evaluate(`
    (() => {
      const rootStyle = getComputedStyle(document.documentElement);
      const body = getComputedStyle(document.body);
      const sidebar = getComputedStyle(document.querySelector(".sidebar"));
      return {
        dataTheme: document.documentElement.getAttribute("data-theme"),
        prefersDark: matchMedia("(prefers-color-scheme: dark)").matches,
        bg: rootStyle.getPropertyValue("--bg").trim(),
        surface: rootStyle.getPropertyValue("--surface").trim(),
        text: rootStyle.getPropertyValue("--text").trim(),
        colorScheme: rootStyle.getPropertyValue("color-scheme").trim(),
        canvas: body.backgroundColor,
        surfaceRgb: sidebar.backgroundColor,
        ink: sidebar.color
      };
    })()
  `);
}

async function main() {
  mkdirSync(shotDir, { recursive: true });
  const userDataDir = mkdtempSync(join(tmpdir(), "wtc-theme-"));
  const { child, wsUrl } = await launchChrome(userDataDir);
  const ws = new WebSocket(wsUrl);
  try {
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("the DevTools socket did not open")), 10000);
      ws.addEventListener("open", () => {
        clearTimeout(timer);
        resolve();
      }, { once: true });
      ws.addEventListener("error", () => {
        clearTimeout(timer);
        reject(new Error("could not open the DevTools socket"));
      }, { once: true });
    });

    const browser = new Cdp(ws);
    const { targetId } = await browser.send("Target.createTarget", { url: "about:blank" });
    const attached = await browser.send("Target.attachToTarget", { targetId, flatten: true });
    const page = new Cdp(ws);
    page.sessionId = attached.sessionId;
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    await page.send("Log.enable");
    // Every case starts from a clean slate. The settings page persists the theme
    // and the language, so a choice made by one case would otherwise still be in
    // effect in the next one — and the "system" case would be measuring the
    // previous case's choice instead of the OS preference.
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: "try { localStorage.clear(); } catch (error) {}"
    });

    const results = [];
    for (const [scheme, choice, expectedAttr, palette] of CASES) {
      await page.send("Emulation.setEmulatedMedia", {
        features: [{ name: "prefers-color-scheme", value: scheme }]
      });
      await page.send("Page.navigate", { url: pageUrl });
      await settle(page);

      if (choice !== "system") {
        await clickButton(page, '[data-view="settings"]');
        await clickButton(page, `[data-theme-choice="${choice}"]`);
      }

      const state = await readState(page);
      const shot = await page.send("Page.captureScreenshot", { format: "png" });
      const name = `${scheme}-${choice}.png`;
      writeFileSync(join(shotDir, name), Buffer.from(shot.data, "base64"));

      const failures = [];
      if (state.prefersDark !== (scheme === "dark")) {
        failures.push(`OS preference not applied (prefersDark=${state.prefersDark})`);
      }
      if (state.dataTheme !== expectedAttr) {
        failures.push(`data-theme=${JSON.stringify(state.dataTheme)}, expected ${JSON.stringify(expectedAttr)}`);
      }
      if (state.bg !== palette.bg) failures.push(`--bg=${state.bg}, expected ${palette.bg}`);
      if (state.surface !== palette.surface) failures.push(`--surface=${state.surface}, expected ${palette.surface}`);
      if (state.text !== palette.text) failures.push(`--text=${state.text}, expected ${palette.text}`);
      if (state.colorScheme !== (palette === DARK ? "dark" : "light")) {
        failures.push(`color-scheme=${state.colorScheme}`);
      }
      if (state.canvas !== palette.canvas) {
        failures.push(`body background=${state.canvas}, expected ${palette.canvas}`);
      }
      if (state.surfaceRgb !== palette.surfaceRgb) {
        failures.push(`sidebar background=${state.surfaceRgb}, expected ${palette.surfaceRgb}`);
      }
      if (state.ink !== palette.ink) {
        failures.push(`sidebar text=${state.ink}, expected ${palette.ink}`);
      }
      results.push({ case: `OS ${scheme} + ${choice}`, name, failures, state });
    }

    let failed = 0;
    for (const result of results) {
      if (result.failures.length === 0) {
        console.log(`PASS  ${result.case.padEnd(22)} -> ${result.name}  (bg ${result.state.bg}, canvas ${result.state.canvas})`);
      } else {
        failed += 1;
        console.log(`FAIL  ${result.case.padEnd(22)} -> ${result.name}`);
        for (const failure of result.failures) console.log(`        ${failure}`);
      }
    }
    console.log(`\n${results.length - failed}/${results.length} combinations correct; screenshots in ${shotDir}`);
    if (failed > 0) process.exitCode = 1;
  } finally {
    ws.close();
    await stopChrome(child);
    rmSync(userDataDir, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});

#!/usr/bin/env node
// Verify the page grid in a real browser: the two things a unit test cannot see.
//
//   1. Every page has the same left and right margin as the worktree page. The
//      text used to start and end in a different place on the welcome, overview,
//      plan, history and settings pages, so switching views moved the page
//      sideways under the reader.
//   2. Nothing spills out of the box it was put in, in either language: an item
//      row stays inside the pane, its name stays on one line, the size and verdict
//      stay beside the text rather than over it, no two rows overlap, and every
//      row carries its reasons. The list pages were seven-column tables once, and a
//      column narrower than its content did not push its neighbour — the content
//      painted over the next column, which is how a verdict badge ended up sliced
//      in half by the reason box beside it. The two tables that remain are checked
//      the same way, cell by cell.
//
// Why a real browser: both are layout questions. happy-dom does not lay out a
// table at all, and a stylesheet assertion cannot know how wide "Must not delete"
// is in the font the machine actually has. A cell's own width against its own
// content is the measurement, in each language, because the UI follows the user's
// choice and the English labels are about a third wider than the Chinese ones.
//
// Both are checked at the app's default window size (the one in
// src-tauri/tauri.conf.json), which is where styles.css is sized to fit. A
// narrower window legitimately truncates a long path; that is a graceful
// degradation, and it is why nothing spills over anything there either.
//
// Usage: node scripts/verify-layout.mjs [path-to-html]   (default dist/preview.html)
// Env:   CHROME_PATH=/path/to/Chrome   (default: the macOS Chrome bundle)

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pagePath = resolve(process.argv[2] ?? join(root, "dist", "preview.html"));
const CHROME = process.env.CHROME_PATH ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const pageUrl = `file://${pagePath}`;

// The default window from src-tauri/tauri.conf.json: the width the column shares
// were computed for, and the only one where "every label fits" is the contract.
const WINDOW = { width: 1180, height: 760 };
const SEND_TIMEOUT_MS = 15_000;

/** The pages to walk, and which one decides the margins the others must match. */
const REFERENCE_VIEW = "worktrees";
const VIEWS = ["welcome", "overview", "worktrees", "artifacts", "hygiene", "plan", "history", "settings"];
const LOCALES = ["zh-CN", "en"];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

class Cdp {
  #ws;
  #next = 0;
  #pending = new Map();
  sessionId = null;

  /** Console errors and uncaught exceptions seen since the page started. */
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
      for (const entry of this.#pending.values()) {
        entry.reject(new Error("the DevTools socket closed before the call was answered"));
      }
      this.#pending.clear();
    });
  }

  #collect(msg) {
    if (msg.method === "Runtime.exceptionThrown") {
      this.diagnostics.push(`uncaught: ${msg.params.exceptionDetails.text}`);
    } else if (msg.method === "Log.entryAdded" && msg.params.entry.level === "error") {
      this.diagnostics.push(`page error: ${msg.params.entry.text}`);
    }
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

  async evaluate(expression, { awaitPromise = false } = {}) {
    const result = await this.send("Runtime.evaluate", { expression, awaitPromise, returnByValue: true });
    if (result.exceptionDetails) {
      throw new Error(`page script failed: ${result.exceptionDetails.text}`);
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
      `--window-size=${WINDOW.width},${WINDOW.height}`,
      "--remote-debugging-port=0",
      // A private profile per run, for the same reason as in verify-theme.mjs.
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

async function stopChrome(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill();
  const deadline = Date.now() + 3000;
  while (Date.now() < deadline && child.exitCode === null && child.signalCode === null) await sleep(50);
  if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
}

/** Wait until the SPA has mounted and is idle for two animation frames. */
async function settle(cdp) {
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    ready = await cdp.evaluate(`document.readyState === "complete" && !!document.querySelector(".nav__item")`);
    if (ready) break;
    await sleep(100);
  }
  if (!ready) {
    throw new Error("the page never finished loading (.nav__item never appeared)");
  }
  await cdp.evaluate(`new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done(null))))`, {
    awaitPromise: true
  });
}

/**
 * Click the one element matching `selector`.
 *
 * By selector rather than by label: the UI follows the system language, so
 * "Settings" is not "设置" on a Chinese machine.
 */
async function click(cdp, selector) {
  const missing = await cdp.evaluate(`
    (() => {
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return "no element matches " + ${JSON.stringify(selector)};
      el.click();
      return null;
    })()
  `);
  if (missing) throw new Error(missing);
  await cdp.evaluate(`new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done(null))))`, {
    awaitPromise: true
  });
}

/**
 * What one page looks like right now.
 *
 * `margins` is the gap between the page and the window edge — the thing the
 * reference page decides. `problems` is everything the layout got wrong, each one
 * naming what it saw:
 *
 *   - an item row wider than the container it sits in, so something inside it
 *     spills sideways;
 *   - a row's name wrapped onto a second line, which is the failure the three-line
 *     row exists to prevent: the name is one line with an ellipsis, never a
 *     paragraph;
 *   - the figures sitting over the text instead of beside it;
 *   - two rows overlapping each other;
 *   - a row with no reason on it, since the row is what carries the evidence now;
 *   - a table cell whose content is wider than the cell (the cleanup plan and the
 *     execution result are still tables).
 *
 * The row measurements are geometric rather than textual because the thing that
 * broke before was geometric: `scrollWidth` against `clientWidth` is the whole
 * question "does this fit where it was put", and it needs no knowledge of what the
 * content says.
 */
const READ_PAGE = `(() => {
  const content = document.querySelector(".content");
  const page = content.firstElementChild;
  const box = page.getBoundingClientRect();
  const problems = [];

  const rows = Array.from(document.querySelectorAll(".item-row"));
  rows.forEach((row, index) => {
    if (row.scrollWidth > row.clientWidth + 1) {
      problems.push(\`row \${index} is \${row.scrollWidth - row.clientWidth}px wider than its container\`);
    }
    const name = row.querySelector(".item-row__name");
    if (name) {
      const lines = Math.round(name.getBoundingClientRect().height / 22);
      if (lines > 1) problems.push(\`row \${index} wraps its name over \${lines} lines\`);
    }
    const main = row.querySelector(".item-row__main");
    const figures = row.querySelector(".item-row__figures");
    if (main && figures) {
      const overlap = main.getBoundingClientRect().right - figures.getBoundingClientRect().left;
      if (overlap > 0.5) problems.push(\`row \${index}: the figures sit \${Math.round(overlap)}px over the text\`);
    }
    // Painted, not present: a hidden reason chip is still in the DOM, and a row
    // with no evidence on it is the failure this is here to catch.
    const reason = row.querySelector(".reason");
    if (!reason || reason.getClientRects().length === 0) problems.push(\`row \${index} shows no reason\`);
  });

  const boxes = rows.map((row) => row.getBoundingClientRect()).sort((a, b) => a.top - b.top);
  for (let index = 1; index < boxes.length; index += 1) {
    if (boxes[index - 1].bottom - boxes[index].top > 1) problems.push("two rows overlap");
  }

  document.querySelectorAll("table.table th, table.table td").forEach((cell) => {
    if (cell.scrollWidth > cell.clientWidth + 1) {
      problems.push(\`a table cell overflows: "\${cell.textContent.trim().slice(0, 18)}" needs \${cell.scrollWidth}px, has \${cell.clientWidth}px\`);
    }
  });

  return {
    margins: { left: Math.round(box.left), right: Math.round(window.innerWidth - box.right) },
    paneOverflowX: content.scrollWidth - content.clientWidth,
    problems
  };
})()`;

async function main() {
  const userDataDir = mkdtempSync(join(tmpdir(), "wtc-layout-"));
  const { child, wsUrl } = await launchChrome(userDataDir);
  const ws = new WebSocket(wsUrl);
  const failures = [];
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
    // The settings page persists the language, so a choice made in one case would
    // still be in effect in the next one.
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: "try { localStorage.clear(); } catch (error) {}"
    });
    await page.send("Emulation.setDeviceMetricsOverride", {
      ...WINDOW,
      deviceScaleFactor: 1,
      mobile: false
    });
    await page.send("Page.navigate", { url: pageUrl });
    await settle(page);

    for (const locale of LOCALES) {
      // The welcome screen is the one page that needs no scan, so its margins are
      // read before one starts; then every other page is read against the
      // worktree page's.
      await click(page, '[data-view="welcome"]');
      const welcome = await page.evaluate(READ_PAGE);

      await page.evaluate(`(() => {
        const input = document.querySelector(".input--mono");
        input.value = "/Users/andy/MySrc/Weee/FastTrack/mms-steel";
        input.dispatchEvent(new Event("input", { bubbles: true }));
      })()`);
      await page.evaluate(`document.querySelector('[data-action="start-scan"]').click()`);
      let scanned = false;
      for (let attempt = 0; attempt < 150; attempt += 1) {
        scanned = await page.evaluate(`!document.querySelector('[data-view="artifacts"]').disabled`);
        if (scanned) break;
        await sleep(200);
      }
      if (!scanned) throw new Error(`the sample scan never finished (${locale})`);

      await click(page, '[data-view="settings"]');
      await click(page, `[data-locale="${locale}"]`);

      await click(page, `[data-view="${REFERENCE_VIEW}"]`);
      const reference = await page.evaluate(READ_PAGE);
      const expected = reference.margins;

      const results = [{ view: "welcome", ...welcome }, { view: REFERENCE_VIEW, ...reference }];
      for (const view of VIEWS) {
        if (view === REFERENCE_VIEW) continue;
        if (view === "welcome") continue;
        const enabled = await page.evaluate(`!document.querySelector('[data-view="${view}"]').disabled`);
        if (!enabled) {
          failures.push(`${locale} ${view}: the page could not be opened`);
          continue;
        }
        await click(page, `[data-view="${view}"]`);
        results.push({ view, ...(await page.evaluate(READ_PAGE)) });
      }

      for (const entry of results) {
        const problems = [...entry.problems];
        if (entry.margins.left !== expected.left || entry.margins.right !== expected.right) {
          problems.push(
            `margins ${entry.margins.left}/${entry.margins.right}, the ${REFERENCE_VIEW} page has ${expected.left}/${expected.right}`
          );
        }
        if (entry.paneOverflowX > 0) problems.push(`the pane scrolls sideways by ${entry.paneOverflowX}px`);
        if (problems.length > 0) {
          failures.push(`${locale} ${entry.view}: ${problems.join(" | ")}`);
        }
      }
      const checked = results.length;
      const failed = results.filter((entry) =>
        failures.some((line) => line.startsWith(`${locale} ${entry.view}:`))
      ).length;
      console.log(`${failed === 0 ? "PASS" : "FAIL"}  ${locale}  ${checked} pages, margins ${expected.left}/${expected.right}`);
    }
  } finally {
    await stopChrome(child);
    rmSync(userDataDir, { recursive: true, force: true });
  }

  if (failures.length > 0) {
    console.error(`\n${failures.length} layout problem(s):`);
    for (const line of failures) console.error(`  - ${line}`);
    process.exit(1);
  }
  console.log("\nevery page has the worktree page's margins, and nothing spills out of its box");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
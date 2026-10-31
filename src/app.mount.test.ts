// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import { createApp, nextTick } from "vue";
import App from "./App.vue";
import { mockSnapshot } from "./mock/snapshot";
import * as state from "./state";

// The other tests exercise modules in isolation; this one mounts the real app.
//
// It exists because a module-level mistake — a throw while settings load, a
// message table read before it is bound, a template that renders `undefined` —
// shows up nowhere else. The browser check (`pnpm test:theme`) would catch it,
// but only where a headless Chrome can actually run, and this runs everywhere.

function mountApp() {
  const host = document.createElement("div");
  document.body.append(host);
  const app = createApp(App);
  app.mount(host);
  return { app, host };
}

/** Let the pending promises behind a click settle. */
function settle() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

describe("the app shell", () => {
  it("mounts and renders the navigation", async () => {
    const { app, host } = mountApp();
    await nextTick();

    const items = host.querySelectorAll(".nav__item");
    expect(items.length).toBeGreaterThan(0);
    // The welcome page is what a fresh start shows.
    expect(host.querySelector(".welcome")).not.toBeNull();

    app.unmount();
    host.remove();
  });

  it("mounts without reaching for the desktop backend", async () => {
    // In a browser there is no Tauri backend, and the shell must still render:
    // every `invoke` is behind the platform layer, which reports that instead of
    // calling into nothing.
    const { app, host } = mountApp();
    await nextTick();

    expect("__TAURI_INTERNALS__" in window).toBe(false);
    // The sidebar says so rather than pretending the pages are live.
    expect(host.textContent).toContain("Browser preview");

    app.unmount();
    host.remove();
  });

  it("answers a click on the folder picker instead of doing nothing", async () => {
    // The button used to fill in a sample path, which is why a click that opened
    // nothing looked like a working button. It now asks the platform for a
    // folder, and where there is no picker it has to say so — a click with no
    // visible effect is the failure this guards against.
    const { app, host } = mountApp();
    await nextTick();

    const button = host.querySelector<HTMLButtonElement>('[data-action="choose-folder"]');
    expect(button, "the folder picker button is missing").not.toBeNull();
    expect(host.querySelector(".field__hint--warn")).toBeNull();

    button?.click();
    await settle();

    expect(host.querySelector(".field__hint--warn")?.textContent?.trim()).toBeTruthy();

    app.unmount();
    host.remove();
  });

  it("keeps the scan button disabled until a path is typed", async () => {
    const { app, host } = mountApp();
    await nextTick();

    const start = host.querySelector<HTMLButtonElement>('[data-action="start-scan"]');
    expect(start?.disabled).toBe(true);

    const field = host.querySelector<HTMLInputElement>(".input--mono");
    expect(field).not.toBeNull();
    field!.value = "/tmp/some-repo";
    field!.dispatchEvent(new Event("input"));
    await nextTick();

    expect(start?.disabled).toBe(false);

    app.unmount();
    host.remove();
  });

  it("gives every hygiene rule a title and every finding a sentence", async () => {
    // The page reads its copy straight from the message table, keyed by rule. A
    // key the table does not have renders an empty title or an empty fix line,
    // which is invisible in a type check but makes the page useless — and the one
    // id that carries a dot (`B1.1`) is the one a dotted key cannot hold.
    const { app, host } = mountApp();
    state.snapshot.value = mockSnapshot;
    state.view.value = "hygiene";
    await nextTick();

    const heads = [...host.querySelectorAll(".group-head")];
    expect(heads.length).toBeGreaterThan(5);
    for (const head of heads) {
      const rule = head.querySelector(".chip")?.textContent?.trim() ?? "";
      const title = head.querySelector(".group-head__path")?.textContent?.trim() ?? "";
      expect(title.length, `${rule} has no title`).toBeGreaterThan(0);
      expect(title, `${rule} fell back to its bare id`).not.toBe(rule);
    }
    // The dotted id is among them, so this covers the mapping and not just the
    // rules whose ids happen to be plain letters and digits.
    expect(heads.map((head) => head.querySelector(".chip")?.textContent?.trim())).toContain("B1.1");

    const text = host.textContent ?? "";
    expect(text).not.toContain("undefined");
    // Each finding carries its own sentence, not just its evidence line.
    expect(text).not.toContain("Fix: \n");

    app.unmount();
    host.remove();
    state.view.value = "welcome";
    state.snapshot.value = null;
  });

  it("does not offer a cleanup action it cannot carry out", async () => {
    const { app, host } = mountApp();
    state.snapshot.value = mockSnapshot;
    state.view.value = "plan";
    await nextTick();

    const button = host.querySelector<HTMLButtonElement>('[data-action="run-cleanup"]');
    expect(button, "the run button is missing").not.toBeNull();
    // A preview cannot reach the file system. Offering the button and failing on press
    // would be the same "a click that does nothing" the picker was rewritten to fix.
    expect(button?.disabled).toBe(true);
    expect(host.querySelector(".notice")?.textContent?.trim()).toBeTruthy();

    state.view.value = "welcome";
    state.snapshot.value = null;
    app.unmount();
    host.remove();
  });
});

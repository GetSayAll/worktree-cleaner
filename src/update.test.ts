// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from "vitest";

// The updater talks to the host, so the host is replaced here. The state machine
// around it is the part worth testing: which phrase the reader sees after which
// outcome, and that a failure never leaves the page claiming an update is on
// its way.

const { check, relaunch } = vi.hoisted(() => ({ check: vi.fn(), relaunch: vi.fn() }));

vi.mock("@tauri-apps/plugin-updater", () => ({ check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch }));

/** A stand-in for the plugin's `Update`: it carries the download and the notes. */
function fakeUpdate(version: string, body = "") {
  const downloadAndInstall = vi.fn(async (onEvent: (event: unknown) => void) => {
    onEvent({ event: "Started", data: { contentLength: 1000 } });
    onEvent({ event: "Progress", data: { chunkLength: 400 } });
    onEvent({ event: "Progress", data: { chunkLength: 600 } });
    onEvent({ event: "Finished" });
  });
  return { version, body, currentVersion: "0.1.0", date: new Date(), downloadAndInstall };
}

/** The module fresh, so no test inherits the update another one found. */
async function loadUpdateModule() {
  vi.resetModules();
  return import("./update");
}

/** Makes the host look like a desktop shell. */
function pretendDesktop() {
  (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {};
}

function pretendBrowser() {
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
}

describe("the update path", () => {
  beforeEach(() => {
    check.mockReset();
    relaunch.mockReset();
    pretendBrowser();
  });

  it("says that a browser preview cannot update itself", async () => {
    const { checkForUpdates, updateState } = await loadUpdateModule();
    await checkForUpdates();
    expect(updateState.phase).toBe("unsupported");
    expect(check).not.toHaveBeenCalled();
  });

  it("reports a newer version when the feed has one", async () => {
    pretendDesktop();
    check.mockResolvedValue(fakeUpdate("0.2.0", "Fixed a thing."));
    const { checkForUpdates, updateState } = await loadUpdateModule();
    await checkForUpdates();
    expect(updateState.phase).toBe("available");
    expect(updateState.version).toBe("0.2.0");
    expect(updateState.notes).toBe("Fixed a thing.");
    expect(updateState.checkedAt).not.toBeNull();
  });

  it("reports that nothing newer exists when the feed has none", async () => {
    pretendDesktop();
    check.mockResolvedValue(null);
    const { checkForUpdates, updateState } = await loadUpdateModule();
    await checkForUpdates();
    expect(updateState.phase).toBe("idle");
    expect(updateState.version).toBeNull();
    expect(updateState.error).toBeNull();
  });

  it("keeps the reason a check failed", async () => {
    pretendDesktop();
    check.mockRejectedValue(new Error("the feed is unreachable"));
    const { checkForUpdates, updateState } = await loadUpdateModule();
    await checkForUpdates();
    expect(updateState.phase).toBe("error");
    expect(updateState.error).toBe("the feed is unreachable");
  });

  it("counts the download and ends ready to restart", async () => {
    pretendDesktop();
    const update = fakeUpdate("0.2.0");
    check.mockResolvedValue(update);
    const { checkForUpdates, installUpdate, updateState, downloadPercent } = await loadUpdateModule();
    await checkForUpdates();
    await installUpdate();
    expect(update.downloadAndInstall).toHaveBeenCalled();
    expect(updateState.phase).toBe("ready");
    expect(updateState.downloaded).toBe(1000);
    expect(downloadPercent()).toBe(100);
  });

  it("does not claim an update is on its way when the installation failed", async () => {
    pretendDesktop();
    const update = fakeUpdate("0.2.0");
    update.downloadAndInstall.mockRejectedValueOnce(new Error("the disk is full"));
    check.mockResolvedValue(update);
    const { checkForUpdates, installUpdate, updateState } = await loadUpdateModule();
    await checkForUpdates();
    await installUpdate();
    expect(updateState.phase).toBe("error");
    expect(updateState.error).toBe("the disk is full");
  });

  it("restarts the app when it is asked to", async () => {
    pretendDesktop();
    relaunch.mockResolvedValue(undefined);
    const { restartApp } = await loadUpdateModule();
    await restartApp();
    expect(relaunch).toHaveBeenCalled();
  });
});

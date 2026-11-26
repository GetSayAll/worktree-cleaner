// @vitest-environment happy-dom
import { createApp, nextTick } from "vue";
import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ reveal: vi.fn() }));

vi.mock("../platform", () => ({
  hostOs: () => "macos" as const,
  isDesktop: () => true,
  revealInFileManager: mocks.reveal
}));

import RevealPathButton from "./RevealPathButton.vue";
import { messages } from "../i18n";

async function mount(path: string) {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const app = createApp(RevealPathButton, { path });
  app.mount(host);
  await nextTick();
  return { app, host };
}

afterEach(() => {
  document.body.innerHTML = "";
  mocks.reveal.mockReset();
});

describe("RevealPathButton", () => {
  it("asks the platform to reveal the exact path", async () => {
    mocks.reveal.mockResolvedValue(undefined);
    const { app, host } = await mount("/tmp/plain-folder");
    host.querySelector("button")?.click();
    await nextTick();
    expect(mocks.reveal).toHaveBeenCalledWith("/tmp/plain-folder");
    app.unmount();
  });

  it("says the item could not be shown without exposing a raw error", async () => {
    mocks.reveal.mockRejectedValue(new Error("fatal: path disappeared"));
    const { app, host } = await mount("/tmp/plain-folder");
    host.querySelector("button")?.click();
    await nextTick();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await nextTick();
    expect(host.textContent).toContain(messages.errors.reveal);
    expect(host.textContent).not.toContain("fatal");
    app.unmount();
  });
});

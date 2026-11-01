import { createApp } from "vue";
import App from "./App.vue";
import { hostOs, isDesktop, systemAccent } from "./platform";
import "./styles.css";

// Which host is drawing the window. The stylesheet needs it for one thing: on
// macOS the window has no title bar (see `titleBarStyle` in tauri.conf.json), so
// the traffic lights float over the top-left corner and the sidebar has to leave
// them a strip to sit in. A browser preview has no lights, which is why the flag is
// only set when there is a desktop shell.
const desktop = isDesktop();
if (desktop) document.documentElement.dataset.host = hostOs();

createApp(App).mount("#app");

/**
 * The accent the operating system is using, applied once the host answers.
 *
 * After the first paint rather than before it: the window is already on screen when
 * this module runs, so waiting for a round trip to the host would buy a correct
 * accent with a longer blank window. What the correction actually changes is the
 * selected nav row and the primary button, so it reads as a tint settling.
 *
 * Set inline on the root element rather than by swapping a stylesheet: an inline
 * declaration is the one thing that outranks `:root[data-theme="dark"]` without a
 * specificity duel, and the shades the stylesheet derives from `--accent` follow it.
 */
void systemAccent().then((accent) => {
  if (accent) document.documentElement.style.setProperty("--accent", accent);
});

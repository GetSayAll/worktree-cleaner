<script setup lang="ts">
/**
 * The one place the navigation buttons live.
 *
 * Every step of the flow needs a way back and a way forward, and until now each
 * page put them wherever its own layout happened to leave room: a return only on
 * some pages, a "continue" on others, and never in the same place twice. A user
 * who learned one page could not rely on the next.
 *
 * The arrangement is fixed: **back on the left, forward on the right**, the
 * selection summary in the middle. That is the order the eye already expects and
 * it does not move between pages, so the buttons are found rather than hunted for.
 */
import { computed } from "vue";
import { goTo } from "../state";
import type { ViewKey } from "../state";
import { messages } from "../i18n";

const props = defineProps<{
  /** Where "back" goes. Omit to have no back button at all. */
  back?: NavKey;
  /** Where "forward" goes. Omit to have no forward button. */
  forward?: NavKey;
  /** Label for the forward button; defaults to the destination's own name. */
  forwardLabel?: string;
  /** Marks the forward button as the action that ends this step. */
  primary?: boolean;
  /** Disables the forward button, e.g. when nothing is selected. */
  forwardDisabled?: boolean;
  /** The middle line: what is selected and what it adds up to. */
  summary?: string;
}>();

/**
 * A view a user can navigate to. `scanning` is excluded: it is a state the app
 * passes through on its own, never a destination, so offering a button back to it
 * would be offering a dead end.
 */
type NavKey = Exclude<ViewKey, "scanning">;

/**
 * The label for each destination, mirrored from the nav table in `App.vue`.
 *
 * Kept as keys rather than captured strings: the language can change while the
 * app is running, so the label has to be read at render time.
 */
const NAV_LABELS: Record<NavKey, "project" | "overview" | "worktrees" | "artifacts" | "hygiene" | "plan" | "result" | "history" | "settings"> = {
  welcome: "project",
  overview: "overview",
  worktrees: "worktrees",
  artifacts: "artifacts",
  hygiene: "hygiene",
  plan: "plan",
  result: "result",
  history: "history",
  settings: "settings"
};

/**
 * The label for "back".
 *
 * Named after the destination rather than generic "back", because the thing a user
 * wants to know is where the button takes them, not that it reverses something.
 */
const backLabel = computed(() => (props.back ? messages.nav[NAV_LABELS[props.back]] : ""));
const nextLabel = computed(() => props.forwardLabel ?? (props.forward ? messages.nav[NAV_LABELS[props.forward]] : ""));

</script>

<template>
  <div class="actionbar">
    <div class="actionbar__side">
      <button v-if="back" class="btn" :data-nav-back="back" @click="goTo(back)">
        {{ backLabel }}
      </button>
    </div>
    <span v-if="summary" class="actionbar__summary muted">{{ summary }}</span>
    <div class="actionbar__side actionbar__side--end">
      <button
        v-if="forward"
        class="btn"
        :class="primary ? 'btn--primary' : ''"
        :disabled="forwardDisabled"
        :data-nav-forward="forward"
        @click="goTo(forward)"
      >
        {{ nextLabel }}
      </button>
    </div>
  </div>
</template>

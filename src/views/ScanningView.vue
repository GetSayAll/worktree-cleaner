<script setup lang="ts">
import { computed } from "vue";
import { cancelScan, projectPath, scanState, SCAN_STAGES } from "../state";
import { scanTrace } from "../mock/snapshot";
import { formatClock } from "../format";
import { isDesktop } from "../platform";
import { messages } from "../i18n";

const desktop = isDesktop();
const visibleTrace = computed(() =>
  scanTrace.slice(Math.max(0, scanState.traceLine - 4), scanState.traceLine + 1)
);

/**
 * The stage copy, looked up while rendering.
 *
 * Keys rather than captured strings: switching languages while this page is open
 * has to relabel the stages, too.
 */
const stages = computed(() => SCAN_STAGES.map((key) => ({ key, ...messages.scan.stages[key] })));

// The desktop scan is one call that answers when it is finished, so the stages
// do not report where the core has reached — they move to show that work is under
// way. Without them the page showed four dead markers beside a running clock and
// read as a hang. The `liveNote` on the page says plainly which it is, so the
// movement is never mistaken for a real progress report.

function stageClass(index: number): string {
  if (index < scanState.stageIndex) return "stage is-done";
  if (index === scanState.stageIndex) return "stage is-active";
  return "stage";
}

function stageMark(index: number): string {
  if (index < scanState.stageIndex) return "✓";
  if (index === scanState.stageIndex) return "●";
  return "○";
}

// The preview's trace lines are sample output. The desktop app has none to show:
// the core answers once, with the whole result, so there is no partial output to
// print and none is invented.
const scannedTrace = desktop ? [] : scanTrace;
</script>

<template>
  <div class="scan">
    <div class="stack stack--tight">
      <h1>{{ messages.scan.title }}</h1>
      <p class="mono faint">{{ projectPath }}</p>
    </div>

    <div class="card">
      <div class="card__body">
        <div class="stage-list">
          <div v-for="(stage, index) in stages" :key="stage.key" :class="stageClass(index)">
            <span class="stage__mark">{{ stageMark(index) }}</span>
            <span>
              <div class="stage__label">{{ stage.label }}</div>
              <div class="stage__detail">{{ stage.detail }}</div>
            </span>
          </div>
        </div>
        <p v-if="desktop" class="field__hint">{{ messages.scan.liveNote }}</p>
      </div>
      <div class="card__foot">
        <div class="spread">
          <span class="faint tiny">{{ messages.scan.elapsed(formatClock(scanState.elapsed)) }}</span>
          <button class="btn" @click="cancelScan">{{ messages.scan.cancel }}</button>
        </div>
      </div>
    </div>

    <div v-if="scannedTrace.length > 0" class="trace">
      <div v-for="line in visibleTrace" :key="line" class="trace__line">{{ line }}</div>
    </div>

    <p class="faint tiny">{{ " " + messages.scan.budgetNote + " " }}</p>
  </div>
</template>

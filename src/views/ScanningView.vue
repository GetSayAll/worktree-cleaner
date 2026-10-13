<script setup lang="ts">
import { computed } from "vue";
import { cancelScan, projectPath, scanState } from "../state";
import { scanStages, scanTrace } from "../mock/snapshot";
import { formatClock } from "../format";

const visibleTrace = computed(() => scanTrace.slice(Math.max(0, scanState.traceLine - 4), scanState.traceLine + 1));

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
</script>

<template>
  <div class="scan">
    <div class="stack stack--tight">
      <h1>Scanning</h1>
      <p class="mono faint">{{ projectPath }}</p>
    </div>

    <div class="card">
      <div class="card__body">
        <div class="stage-list">
          <div v-for="(stage, index) in scanStages" :key="stage.key" :class="stageClass(index)">
            <span class="stage__mark">{{ stageMark(index) }}</span>
            <span>
              <div class="stage__label">{{ stage.label }}</div>
              <div class="stage__detail">{{ stage.detail }}</div>
            </span>
          </div>
        </div>
      </div>
      <div class="card__foot">
        <div class="spread">
          <span class="faint tiny">Elapsed {{ formatClock(scanState.elapsed) }}</span>
          <button class="btn" @click="cancelScan">Cancel</button>
        </div>
      </div>
    </div>

    <div class="trace">
      <div v-for="line in visibleTrace" :key="line" class="trace__line">{{ line }}</div>
    </div>

    <p class="faint tiny">
      Each directory probe has a timeout and the whole scan has a total budget; if the budget runs out it honestly reports "scan incomplete" and marks the affected items as "Needs review" instead of filling them in silently.
    </p>
  </div>
</template>

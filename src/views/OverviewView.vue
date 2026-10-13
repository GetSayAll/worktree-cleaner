<script setup lang="ts">
import { computed } from "vue";
import { artifacts, counts, goTo, primaryReason, selectedSummary, snapshot, worktrees } from "../state";
import SafetyBadge from "../components/SafetyBadge.vue";
import StatCard from "../components/StatCard.vue";
import { formatBytes, formatCount, formatRelTime, displayPath } from "../format";
import { messages } from "../i18n";

const root = computed(() => snapshot.value?.root ?? "");

const topWorktrees = computed(() =>
  [...worktrees.value].sort((a, b) => (b.size_bytes ?? 0) - (a.size_bytes ?? 0)).slice(0, 5)
);

const topArtifacts = computed(() =>
  [...artifacts.value].sort((a, b) => (b.size_bytes ?? 0) - (a.size_bytes ?? 0)).slice(0, 5)
);

const safeBytes = computed(() =>
  worktrees.value.filter((w) => w.safety.status === "Safe").reduce((sum, w) => sum + (w.size_bytes ?? 0), 0) +
  artifacts.value.filter((a) => a.safety.status === "Safe").reduce((sum, a) => sum + (a.size_bytes ?? 0), 0)
);

const reviewCount = computed(() => counts.value.worktree.Review + counts.value.artifact.Review);
const unsafeCount = computed(() => counts.value.worktree.Unsafe + counts.value.artifact.Unsafe);
const selectionCount = computed(() => selectedSummary.value.worktrees + selectedSummary.value.artifacts);
</script>

<template>
  <div class="stack">
    <div class="spread">
      <div>
        <h1>{{ messages.overview.title }}</h1>
        <p class="faint tiny mono">{{ messages.overview.subtitle(root, snapshot?.default_branch || messages.common.unknown) }}</p>
      </div>
      <div class="row-wrap">
        <button class="btn" @click="goTo('worktrees')">{{ messages.overview.viewWorktrees }}</button>
        <button class="btn" @click="goTo('artifacts')">{{ messages.overview.viewArtifacts }}</button>
      </div>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard
        :label="messages.overview.stats.safeLabel"
        tone="safe"
        value-tone
        :value="String(counts.worktree.Safe + counts.artifact.Safe)"
        :sub="messages.overview.stats.safeSub(formatBytes(safeBytes))"
      />
      <StatCard
        :label="messages.overview.stats.reviewLabel"
        tone="review"
        value-tone
        :value="String(reviewCount)"
        :sub="messages.overview.stats.reviewSub"
      />
      <StatCard
        :label="messages.overview.stats.unsafeLabel"
        tone="unsafe"
        value-tone
        :value="String(unsafeCount)"
        :sub="messages.overview.stats.unsafeSub"
      />
      <StatCard
        :label="messages.overview.stats.reclaimableLabel"
        tone="accent"
        :value="formatBytes(selectedSummary.bytes)"
        :sub="messages.overview.stats.reclaimableSub(selectionCount, selectedSummary.worktrees, selectedSummary.artifacts)"
      />
    </div>

    <div v-for="warning in snapshot?.warnings || []" :key="warning" class="banner banner--warn">
      <span>⚠</span>
      <span>
        <span class="banner__title">{{ messages.overview.warningTitle }}</span>{{ warning }}
      </span>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.overview.largestWorktrees }}</h2>
        <button class="btn btn--ghost" @click="goTo('worktrees')">{{ messages.overview.viewAll(worktrees.length) }}</button>
      </div>
      <div class="card__body card__body--flush">
        <table class="table">
          <tbody>
            <tr v-for="item in topWorktrees" :key="item.path">
              <td>
                <div class="mono">{{ displayPath(item.path, root) }}</div>
                <div class="tiny faint">{{ item.branch || messages.common.detached }}{{ item.is_main ? messages.overview.mainWorktreeSuffix : "" }}</div>
              </td>
              <td class="nowrap">
                <span class="chip">{{ item.agent || messages.common.unknownAgent }}</span>
              </td>
              <td class="cell-size">
                <div class="numeric">{{ formatBytes(item.size_bytes) }}</div>
                <div class="tiny faint">{{ formatRelTime(item.last_updated_at) }}</div>
              </td>
              <td class="nowrap"><SafetyBadge :status="item.safety.status" /></td>
              <td class="cell-size tiny faint" style="max-width: 200px">
                {{ primaryReason(item.safety.reasons)?.text }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.overview.largestArtifacts }}</h2>
        <button class="btn btn--ghost" @click="goTo('artifacts')">{{ messages.overview.viewAll(artifacts.length) }}</button>
      </div>
      <div class="card__body card__body--flush">
        <table class="table">
          <tbody>
            <tr v-for="item in topArtifacts" :key="item.path">
              <td>
                <div class="row-wrap">
                  <span class="chip">{{ item.kind }}</span>
                  <span class="mono">{{ displayPath(item.path, root) }}</span>
                </div>
                <div class="tiny faint">{{ messages.overview.artifactMeta(item.ecosystem, formatCount(item.file_count)) }}</div>
              </td>
              <td class="cell-size">
                <div class="numeric">{{ formatBytes(item.size_bytes) }}</div>
                <div class="tiny faint">{{ formatRelTime(item.last_updated_at) }}</div>
              </td>
              <td class="nowrap"><SafetyBadge :status="item.safety.status" /></td>
              <td class="cell-size tiny faint" style="max-width: 200px">{{ primaryReason(item.safety.reasons)?.text }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="actionbar">
      <span :class="selectionCount > 0 ? 'muted' : 'faint'">{{ " " + messages.overview.selected(selectionCount) }} <strong>{{ formatBytes(selectedSummary.bytes) }}</strong></span>
      <span v-if="selectionCount === 0" class="faint tiny">{{ messages.overview.selectHint }}</span>
      <span style="flex: 1"></span>
      <button class="btn" :disabled="selectionCount === 0" @click="goTo('worktrees')">{{ messages.overview.adjustSelection }}</button>
      <button class="btn btn--primary" :disabled="selectionCount === 0" @click="goTo('plan')">{{ messages.overview.generatePlan }}</button>
    </div>
  </div>
</template>

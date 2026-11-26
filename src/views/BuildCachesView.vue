<script setup lang="ts">
import { computed, onMounted } from "vue";
import {
  buildCacheAcknowledged,
  buildCacheError,
  buildCacheLastResult,
  buildCachePlan,
  buildCacheResultSummary,
  buildCacheRoot,
  buildCacheSample,
  buildCacheScanning,
  buildCacheSelectedSummary,
  buildCacheSnapshot,
  executing,
  executeBuildCacheCleanup,
  executeErrorMessage,
  goTo,
  lastWorkspaceRoot,
  logError,
  projectPath,
  recentProjects,
  selectedBuildCaches,
  settings,
  startBuildCacheScan,
  toggleBuildCache,
  toggleGroupBuildCaches,
  workspaceSnapshot
} from "../state";
import SafetyBadge from "../components/SafetyBadge.vue";
import ReasonList from "../components/ReasonList.vue";
import RevealPathButton from "../components/RevealPathButton.vue";
import StatCard from "../components/StatCard.vue";
import {
  compareSizeNullsLast,
  detailText,
  formatBytes,
  formatCount,
  formatRelTime,
  homePath,
  isUnder
} from "../format";
import { messages } from "../i18n";
import { isDesktop, openTrash, pickDirectory } from "../platform";

const desktop = isDesktop();
const records = computed(() => buildCacheSnapshot.value?.records ?? []);
const sortedResults = computed(() =>
  [...buildCacheLastResult.value].sort(
    (a, b) => compareSizeNullsLast(a.size_bytes, b.size_bytes) || a.target.localeCompare(b.target)
  )
);
const visibleRecords = computed(() => {
  const minBytes = settings.min_artifact_mb * 1024 * 1024;
  return records.value
    .filter(
      (record) => record.size_bytes === null || record.size_bytes >= minBytes || record.safety.status === "Unsafe"
    )
    .sort(
      (a, b) => compareSizeNullsLast(a.size_bytes, b.size_bytes) || a.path.localeCompare(b.path)
    );
});
const selected = computed(() =>
  records.value.filter((record) => selectedBuildCaches.value.includes(record.path))
);
const selectableSafes = computed(() =>
  records.value.filter((record) => record.safety.status === "Safe").map((record) => record.path)
);
const allSafeSelected = computed(
  () =>
    selectableSafes.value.length > 0 &&
    selectableSafes.value.every((path) => selectedBuildCaches.value.includes(path))
);
const totalCount = computed(() => visibleRecords.value.length);
const totalBytes = computed(() =>
  visibleRecords.value.reduce((sum, record) => sum + (record.size_bytes ?? 0), 0)
);
const totalUnknown = computed(
  () => visibleRecords.value.filter((record) => record.size_bytes === null).length
);
const hasPlan = computed(
  () => buildCachePlan.value.trash_moves.length > 0 || buildCachePlan.value.blockers.length > 0
);
const canRun = computed(
  () =>
    desktop &&
    !buildCacheScanning.value &&
    !executing.value &&
    buildCachePlan.value.blockers.length === 0 &&
    buildCachePlan.value.trash_moves.length > 0 &&
    (buildCachePlan.value.ack_reasons.length === 0 || buildCacheAcknowledged.value)
);

onMounted(() => {
  // The page does not ask the reader to know which folder holds a cache. It
  // starts from the workspace the repository was opened from, then from a recent
  // folder that contains the project; only when neither exists does it fall back
  // to the project itself or the folder picker.
  const project = projectPath.value;
  const knownWorkspace =
    lastWorkspaceRoot.value && (!project || isUnder(project, lastWorkspaceRoot.value))
      ? lastWorkspaceRoot.value
      : "";
  const ancestor = recentProjects.value
    .map((entry) => entry.path)
    .filter((path) => path !== project && project !== "" && isUnder(project, path))
    .sort((left, right) => left.length - right.length)[0];
  const preferred = workspaceSnapshot.value?.root ?? knownWorkspace ?? ancestor ?? "";
  const previousRoot = buildCacheRoot.value;
  if (!buildCacheRoot.value || (project && previousRoot === project && preferred)) {
    buildCacheRoot.value = preferred || project || recentProjects.value[0]?.path || "";
  }
  if (buildCacheRoot.value !== previousRoot) {
    buildCacheSnapshot.value = null;
    selectedBuildCaches.value = [];
  }
  if (!buildCacheSnapshot.value && buildCacheRoot.value) void startBuildCacheScan();
});

async function chooseFolder() {
  try {
    const picked = await pickDirectory(messages.buildCaches.chooseFolder);
    if (picked !== null) {
      buildCacheRoot.value = picked;
      void startBuildCacheScan();
    }
  } catch {
    // The page's normal scan error covers a failed picker; the folder can also be
    // changed from the workspace by rescanning there.
  }
}

function scan() {
  void startBuildCacheScan();
}

async function openSystemTrash() {
  try {
    await openTrash();
  } catch {
    // The result page reports a failure; this page's action is optional.
  }
}
</script>

<template>
  <div class="stack page--wide">
    <div class="spread">
      <div>
        <h1>{{ messages.buildCaches.title }}</h1>
        <p class="faint tiny">{{ messages.buildCaches.lede }}</p>
      </div>
      <div class="row-wrap">
        <span v-if="buildCacheSample" class="badge badge--accent">{{ messages.app.sampleData }}</span>
        <button class="btn" :disabled="buildCacheScanning || executing" @click="scan">
          {{ buildCacheScanning ? messages.buildCaches.scanning : messages.buildCaches.rescan }}
        </button>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.buildCaches.scopeTitle }}</h2>
        <span class="faint tiny">{{ messages.buildCaches.scopeHint }}</span>
      </div>
      <div class="card__body">
        <div class="row-wrap">
          <span class="mono">{{ buildCacheRoot ? homePath(buildCacheRoot) : messages.buildCaches.noScope }}</span>
          <button class="btn" :disabled="buildCacheScanning || executing" @click="chooseFolder">
            {{ messages.buildCaches.chooseFolder }}
          </button>
        </div>
        <p v-if="!buildCacheRoot.trim()" class="field__hint">{{ messages.buildCaches.emptyRoot }}</p>
        <p v-else-if="buildCacheScanning" class="field__hint">{{ messages.buildCaches.scanning }}</p>
      </div>
    </div>

    <p class="rules-summary"><strong>{{ messages.rules.shortLabel }}</strong>{{ messages.buildCaches.rulesSummary }}</p>

    <div v-if="!desktop" class="notice">{{ messages.preview.notice }}</div>
    <div v-if="buildCacheError" class="notice notice--unsafe">{{ messages.buildCaches.scanFailed }}</div>
    <div v-if="executeErrorMessage" class="notice notice--unsafe">{{ executeErrorMessage }}</div>
    <p v-if="logError" class="notice notice--unsafe">{{ messages.errors.logWrite(logError) }}</p>

    <div class="stat-grid">
      <StatCard
        :label="messages.buildCaches.stats.caches"
        :value="String(totalCount)"
        :sub="messages.buildCaches.stats.cachesSub"
      />
      <StatCard
        :label="messages.buildCaches.stats.bytes"
        tone="accent"
        :value="formatBytes(totalBytes, { approximate: totalUnknown > 0 })"
        :sub="totalUnknown > 0 ? messages.buildCaches.stats.bytesUnknown(totalUnknown) : messages.buildCaches.stats.bytesSub"
      />
      <StatCard
        :label="messages.buildCaches.stats.selected"
        tone="safe"
        value-tone
        :value="String(buildCacheSelectedSummary.count)"
        :sub="messages.buildCaches.stats.selectedSub(formatBytes(buildCacheSelectedSummary.bytes, { approximate: buildCacheSelectedSummary.unknown > 0 }))"
      />
    </div>

    <div v-for="warning in buildCacheSnapshot?.warnings || []" :key="warning" class="banner banner--warn">
      <span>⚠</span>
      <span>{{ warning }}</span>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.buildCaches.inventoryTitle }}</h2>
        <span class="faint tiny">{{ messages.buildCaches.inventoryHint }}</span>
      </div>
      <div class="card__body card__body--flush">
        <div class="select-all">
          <input
            class="check"
            type="checkbox"
            :checked="allSafeSelected"
            :disabled="selectableSafes.length === 0 || executing"
            @change="toggleGroupBuildCaches(selectableSafes, !allSafeSelected)"
          />
          <span>{{ messages.buildCaches.selectAllSafe }}</span>
        </div>

        <div v-for="record in visibleRecords" :key="record.path" class="item-row">
          <div class="item-row__check">
            <input
              class="check"
              type="checkbox"
              :checked="selectedBuildCaches.includes(record.path)"
              :disabled="record.safety.status === 'Unsafe' || executing"
              :title="record.safety.status === 'Unsafe' ? messages.common.unsafeTooltip : ''"
              @change="toggleBuildCache(record.path)"
            />
          </div>
          <div class="item-row__main">
            <div class="item-row__head">
              <span class="chip">{{ record.kind }}</span>
              <span class="item-row__name" :title="record.path">{{ homePath(record.path) }}</span>
            </div>
            <div class="item-row__meta">
              <span>
                {{
                  record.size_bytes === null && record.file_count === 0
                    ? messages.common.unknown
                    : `${formatCount(record.file_count)} ${messages.buildCaches.files}`
                }}
              </span>
              <span>·</span>
              <span>{{ formatRelTime(record.last_updated_at) }}</span>
            </div>
            <div class="item-row__foot">
              <div class="item-row__reasons">
                <ReasonList :reasons="record.safety.reasons" />
              </div>
              <RevealPathButton v-if="record.safety.status !== 'Safe'" :path="record.path" />
            </div>
          </div>
          <div class="item-row__figures">
            <span class="item-row__size">{{ formatBytes(record.size_bytes) }}</span>
            <SafetyBadge :status="record.safety.status" />
          </div>
        </div>

        <div v-if="visibleRecords.length === 0" class="empty">
          {{ messages.buildCaches.empty }}
        </div>
      </div>
    </div>

    <div v-if="hasPlan" class="card">
      <div class="card__head">
        <h2>{{ messages.buildCaches.planTitle }}</h2>
        <span class="faint tiny">{{ messages.buildCaches.planHint }}</span>
      </div>
      <div class="card__body">
        <p class="faint tiny">{{ messages.buildCaches.planLede }}</p>
        <p v-if="selected.length === 0" class="faint">{{ messages.buildCaches.noSelection }}</p>
        <div v-for="record in selected" :key="record.path" class="row-wrap" style="padding: 6px 0">
          <span class="chip">{{ record.kind }}</span>
          <span class="mono">{{ homePath(record.path) }}</span>
          <span class="numeric faint">{{ formatBytes(record.size_bytes) }}</span>
        </div>
        <p class="faint tiny">
          {{ messages.buildCaches.planTotal(formatBytes(buildCachePlan.total_bytes, { approximate: buildCachePlan.total_bytes_unknown > 0 })) }}
        </p>
        <div v-for="blocker in buildCachePlan.blockers" :key="blocker" class="banner banner--danger">
          <span>✕</span><span>{{ blocker }}</span>
        </div>
        <label v-if="buildCachePlan.ack_reasons.length > 0" class="confirm" style="padding-top: 8px">
          <input v-model="buildCacheAcknowledged" class="check check--confirm" type="checkbox" :disabled="executing" />
          <span class="confirm__body">
            <span class="confirm__eyebrow">{{ buildCacheAcknowledged ? messages.plan.confirmed : messages.plan.confirmRequired }}</span>
            <span class="confirm__text">{{ messages.buildCaches.ackConfirm }}</span>
            <span v-if="!buildCacheAcknowledged" class="confirm__hint">{{ messages.plan.confirmHint }}</span>
          </span>
        </label>
        <div class="actionbar__side" style="justify-content: flex-end; padding-top: 8px">
          <button class="btn btn--danger" :disabled="!canRun" @click="executeBuildCacheCleanup">
            {{ executing ? messages.buildCaches.running : messages.buildCaches.execute }}
          </button>
        </div>
      </div>
    </div>

    <div v-if="buildCacheLastResult.length > 0" class="card">
      <div class="card__head">
        <h2>{{ messages.buildCaches.resultTitle }}</h2>
        <button class="btn" @click="openSystemTrash">{{ messages.buildCaches.openTrash }}</button>
      </div>
      <div class="card__body">
        <p class="muted">
          {{ messages.buildCaches.resultSummary(buildCacheResultSummary.trashed, formatBytes(buildCacheResultSummary.trash_bytes, { approximate: buildCacheResultSummary.trash_unknown > 0 }), buildCacheResultSummary.failed) }}
        </p>
        <table class="table">
          <thead>
            <tr>
              <th class="cell-target">{{ messages.buildCaches.columns.directory }}</th>
              <th class="cell-detail">{{ messages.buildCaches.columns.detail }}</th>
              <th class="cell-outcome">{{ messages.buildCaches.columns.result }}</th>
              <th class="cell-size cell-bytes">{{ messages.buildCaches.columns.size }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(item, index) in sortedResults" :key="`${item.target}-${index}`">
              <td class="cell-target"><div class="row-wrap"><span class="mono">{{ homePath(item.target) }}</span></div></td>
              <td class="cell-detail tiny faint">{{ detailText(item.detail_code, item.detail_args, item.detail) }}</td>
              <td class="cell-outcome"><span class="outcome" :class="`outcome--${item.result}`">{{ messages.common.outcome[item.result] }}</span></td>
              <td class="cell-size cell-bytes numeric">{{ formatBytes(item.size_bytes) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="actionbar">
      <div class="actionbar__side">
        <button class="btn" @click="goTo('history')">{{ messages.buildCaches.history }}</button>
      </div>
      <span class="actionbar__summary faint tiny">{{ messages.buildCaches.footer }}</span>
      <div class="actionbar__side" style="margin-left: auto">
        <button class="btn" @click="goTo('welcome')">{{ messages.buildCaches.backToProject }}</button>
      </div>
    </div>
  </div>
</template>

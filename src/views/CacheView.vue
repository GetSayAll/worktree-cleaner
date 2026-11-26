<script setup lang="ts">
import { computed, onMounted } from "vue";
import {
  cacheAcknowledged,
  cacheError,
  cacheLastResult,
  cachePlan,
  cacheResultSummary,
  cacheSample,
  cacheScanning,
  cacheSelectedSummary,
  cacheSnapshot,
  executing,
  executeCacheCleanup,
  executeErrorMessage,
  goTo,
  logError,
  selectedCaches,
  settings,
  startCacheScan,
  toggleCache,
  toggleGroupCaches
} from "../state";
import SafetyBadge from "../components/SafetyBadge.vue";
import ReasonList from "../components/ReasonList.vue";
import RevealPathButton from "../components/RevealPathButton.vue";
import StatCard from "../components/StatCard.vue";
import { compareSizeNullsLast, detailText, formatBytes, formatCount, formatRelTime, homePath } from "../format";
import { messages } from "../i18n";
import { isDesktop, openTrash } from "../platform";

const desktop = isDesktop();
const records = computed(() => cacheSnapshot.value?.records ?? []);
const sortedCacheResults = computed(() =>
  [...cacheLastResult.value].sort(
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
const selected = computed(() => records.value.filter((record) => selectedCaches.value.includes(record.path)));
const selectableSafes = computed(() =>
  records.value.filter((record) => record.safety.status === "Safe").map((record) => record.path)
);
const allSafeSelected = computed(
  () =>
    selectableSafes.value.length > 0 &&
    selectableSafes.value.every((path) => selectedCaches.value.includes(path))
);
const totalCount = computed(() => visibleRecords.value.length);
const totalBytes = computed(() =>
  visibleRecords.value.reduce((sum, record) => sum + (record.size_bytes ?? 0), 0)
);
const totalUnknown = computed(
  () => visibleRecords.value.filter((record) => record.size_bytes === null).length
);
function isUnknownOwner(record: { safety: { reasons: Array<{ code: string }> } }): boolean {
  return record.safety.reasons.some((reason) => reason.code === "af.unknown_cache");
}

const hasPlan = computed(() => cachePlan.value.trash_moves.length > 0 || cachePlan.value.blockers.length > 0);
const canRun = computed(
  () =>
    desktop &&
    !cacheScanning.value &&
    !executing.value &&
    cachePlan.value.blockers.length === 0 &&
    cachePlan.value.trash_moves.length > 0 &&
    (cachePlan.value.ack_reasons.length === 0 || cacheAcknowledged.value)
);

onMounted(() => {
  if (!cacheSnapshot.value) void startCacheScan();
});

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
        <h1>{{ messages.caches.title }}</h1>
        <p class="faint tiny">{{ messages.caches.lede }}</p>
      </div>
      <div class="row-wrap">
        <span v-if="cacheSample" class="badge badge--accent">{{ messages.app.sampleData }}</span>
        <button class="btn" :disabled="cacheScanning || executing" @click="startCacheScan">
          {{ cacheScanning ? messages.caches.scanning : messages.caches.rescan }}
        </button>
      </div>
    </div>

    <p class="rules-summary"><strong>{{ messages.rules.shortLabel }}</strong>{{ messages.caches.rulesSummary }}</p>

    <div v-if="!desktop" class="notice">{{ messages.preview.notice }}</div>
    <div v-if="cacheError" class="notice notice--unsafe">{{ messages.caches.scanFailed }}</div>
    <div v-if="executeErrorMessage" class="notice notice--unsafe">{{ executeErrorMessage }}</div>
    <p v-if="logError" class="notice notice--unsafe">{{ messages.errors.logWrite(logError) }}</p>

    <div class="stat-grid">
      <StatCard
        :label="messages.caches.stats.directories"
        :value="String(totalCount)"
        :sub="messages.caches.stats.directoriesSub"
      />
      <StatCard
        :label="messages.caches.stats.bytes"
        tone="accent"
        :value="formatBytes(totalBytes, { approximate: totalUnknown > 0 })"
        :sub="totalUnknown > 0 ? messages.caches.stats.bytesUnknown(totalUnknown) : messages.caches.stats.bytesSub"
      />
      <StatCard
        :label="messages.caches.stats.selected"
        tone="safe"
        value-tone
        :value="String(cacheSelectedSummary.count)"
        :sub="messages.caches.stats.selectedSub(formatBytes(cacheSelectedSummary.bytes, { approximate: cacheSelectedSummary.unknown > 0 }))"
      />
    </div>

    <div v-for="warning in cacheSnapshot?.warnings || []" :key="warning" class="banner banner--warn">
      <span>⚠</span>
      <span>{{ warning }}</span>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.caches.inventoryTitle }}</h2>
        <span class="faint tiny">{{ messages.caches.inventoryHint }}</span>
      </div>
      <div class="card__body card__body--flush">
        <div class="select-all">
          <input
            class="check"
            type="checkbox"
            :checked="allSafeSelected"
            :disabled="selectableSafes.length === 0 || executing"
            @change="toggleGroupCaches(selectableSafes, !allSafeSelected)"
          />
          <span>{{ messages.caches.selectAllKnown }}</span>
        </div>

        <div v-for="record in records" :key="record.path" class="item-row">
          <div class="item-row__check">
            <input
              class="check"
              type="checkbox"
              :checked="selectedCaches.includes(record.path)"
              :disabled="record.safety.status === 'Unsafe' || executing"
              :title="record.safety.status === 'Unsafe' ? messages.common.unsafeTooltip : ''"
              @change="toggleCache(record.path)"
            />
          </div>
          <div class="item-row__main">
            <div class="item-row__head">
              <span class="chip">{{ record.kind }}</span>
              <span class="item-row__name" :title="record.path">{{ homePath(record.path) }}</span>
              <span v-if="isUnknownOwner(record)" class="chip chip--outside">{{ messages.caches.unknownOwner }}</span>
            </div>
            <div class="item-row__meta">
              <span>
                {{
                  record.size_bytes === null && record.file_count === 0
                    ? messages.common.unknown
                    : `${formatCount(record.file_count)} ${messages.caches.files}`
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
          {{ messages.caches.empty }}
        </div>
      </div>
    </div>

    <div v-if="hasPlan" class="card">
      <div class="card__head">
        <h2>{{ messages.caches.planTitle }}</h2>
        <span class="faint tiny">{{ messages.caches.planHint }}</span>
      </div>
      <div class="card__body">
        <p class="faint tiny">{{ messages.caches.planLede }}</p>
        <p v-if="selected.length === 0" class="faint">{{ messages.caches.noSelection }}</p>
        <div v-for="record in selected" :key="record.path" class="row-wrap" style="padding: 6px 0">
          <span class="chip">{{ record.kind }}</span>
          <span class="mono">{{ homePath(record.path) }}</span>
          <span class="numeric faint">{{ formatBytes(record.size_bytes) }}</span>
        </div>
        <p class="faint tiny">
          {{ messages.caches.planTotal(formatBytes(cachePlan.total_bytes, { approximate: cachePlan.total_bytes_unknown > 0 })) }}
        </p>
        <div v-for="blocker in cachePlan.blockers" :key="blocker" class="banner banner--danger">
          <span>✕</span><span>{{ blocker }}</span>
        </div>
        <label v-if="cachePlan.ack_reasons.length > 0" class="confirm" style="padding-top: 8px">
          <input v-model="cacheAcknowledged" class="check check--confirm" type="checkbox" :disabled="executing" />
          <span class="confirm__body">
            <span class="confirm__eyebrow">{{ cacheAcknowledged ? messages.plan.confirmed : messages.plan.confirmRequired }}</span>
            <span class="confirm__text">{{ messages.caches.ackConfirm }}</span>
            <span v-if="!cacheAcknowledged" class="confirm__hint">{{ messages.plan.confirmHint }}</span>
          </span>
        </label>
        <div class="actionbar__side" style="justify-content: flex-end; padding-top: 8px">
          <button class="btn btn--danger" :disabled="!canRun" @click="executeCacheCleanup">
            {{ executing ? messages.caches.running : messages.caches.execute }}
          </button>
        </div>
      </div>
    </div>

    <div v-if="cacheLastResult.length > 0" class="card">
      <div class="card__head">
        <h2>{{ messages.caches.resultTitle }}</h2>
        <button class="btn" @click="openSystemTrash">{{ messages.caches.openTrash }}</button>
      </div>
      <div class="card__body">
        <p class="muted">
          {{ messages.caches.resultSummary(cacheResultSummary.trashed, formatBytes(cacheResultSummary.trash_bytes, { approximate: cacheResultSummary.trash_unknown > 0 }), cacheResultSummary.failed) }}
        </p>
        <table class="table">
          <thead>
            <tr>
              <th class="cell-target">{{ messages.caches.columns.directory }}</th>
              <th class="cell-detail">{{ messages.caches.columns.detail }}</th>
              <th class="cell-outcome">{{ messages.caches.columns.result }}</th>
              <th class="cell-size cell-bytes">{{ messages.caches.columns.size }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(item, index) in sortedCacheResults" :key="`${item.target}-${index}`">
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
        <button class="btn" @click="goTo('history')">{{ messages.caches.history }}</button>
      </div>
      <span class="actionbar__summary faint tiny">{{ messages.caches.footer }}</span>
      <div class="actionbar__side" style="margin-left: auto">
        <button class="btn" @click="goTo('welcome')">{{ messages.caches.backToProject }}</button>
      </div>
    </div>
  </div>
</template>

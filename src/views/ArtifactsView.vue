<script setup lang="ts">
import { computed, ref } from "vue";
import type { ArtifactRecord } from "../types";
import {
  bundledOwner,
  goTo,
  isSelectedArtifact,
  selectedSummary,
  settings,
  snapshot,
  statusCount,
  toggleArtifact,
  toggleGroupArtifacts,
  visibleArtifacts
} from "../state";
import SafetyBadge from "../components/SafetyBadge.vue";
import ReasonList from "../components/ReasonList.vue";
import { formatBytes, formatCount, formatRelTime, shortenPath } from "../format";
import { messages } from "../i18n";

const sortMode = ref<"size" | "time">("size");
const collapsed = ref<string[]>([]);

const root = computed(() => snapshot.value?.root ?? "");

const groups = computed(() => {
  const buckets = new Map<string, ArtifactRecord[]>();
  for (const item of visibleArtifacts.value) {
    const key = item.worktree_path ?? messages.artifacts.unowned;
    const list = buckets.get(key) ?? [];
    list.push(item);
    buckets.set(key, list);
  }
  const keys = [...buckets.keys()].sort((a, b) => {
    const total = (key: string) => (buckets.get(key) ?? []).reduce((sum, a) => sum + (a.size_bytes ?? 0), 0);
    return total(b) - total(a);
  });
  return keys.map((key) => {
    const items = [...(buckets.get(key) ?? [])].sort((a, b) =>
      sortMode.value === "size" ? (b.size_bytes ?? 0) - (a.size_bytes ?? 0) : (b.last_updated_at ?? "").localeCompare(a.last_updated_at ?? "")
    );
    return {
      key,
      label: key === root.value ? messages.common.mainWorktree(shortenPath(key, root.value)) : shortBounded(key),
      items,
      bytes: items.reduce((sum, a) => sum + (a.size_bytes ?? 0), 0),
      selectable: items.filter((a) => a.safety.status !== "Unsafe" && bundledOwner(a) === null).map((a) => a.path)
    };
  });
});

function shortBounded(path: string): string {
  if (path.startsWith(`${root.value}--`)) return messages.artifacts.worktreeSuffix(path.slice(root.value.length - 1));
  return shortenPath(path, root.value);
}

function isCollapsed(key: string): boolean {
  return collapsed.value.includes(key);
}

function toggleCollapsed(key: string) {
  collapsed.value = isCollapsed(key) ? collapsed.value.filter((k) => k !== key) : [...collapsed.value, key];
}

function groupAllSelected(paths: string[]): boolean {
  return paths.length > 0 && paths.every((p) => isSelectedArtifact(p));
}

function gitState(item: ArtifactRecord): string {
  if (item.git_tracked) return messages.artifacts.gitTracked;
  if (item.git_ignored) return messages.artifacts.gitIgnored;
  if (item.git_ignored === null) return messages.artifacts.gitUnverified;
  return messages.artifacts.gitUntracked;
}

function sizeText(item: ArtifactRecord): string {
  return formatBytes(item.size_bytes, { approximate: item.size_bytes === null && item.file_count > 0 });
}
</script>

<template>
  <div class="stack">
    <div class="spread">
      <h1>{{ messages.artifacts.title }}</h1>
      <span class="faint tiny">{{ " " + messages.artifacts.caption + " " }}</span>
    </div>

    <div class="row-wrap">
      <div class="segmented">
        <button :class="sortMode === 'size' ? 'is-active' : ''" @click="sortMode = 'size'">{{ messages.artifacts.bySize }}</button>
        <button :class="sortMode === 'time' ? 'is-active' : ''" @click="sortMode = 'time'">{{ messages.artifacts.byLastActivity }}</button>
      </div>
      <div class="row-wrap" style="flex: 1; min-width: 240px">
        <span class="faint tiny nowrap">{{ messages.artifacts.showOnlyLargerThan }}</span>
        <input v-model.number="settings.min_artifact_mb" class="slider" type="range" min="0" max="500" step="10" style="max-width: 200px" />
        <span class="tiny numeric nowrap">{{ settings.min_artifact_mb }} {{ messages.units.megabytes }}</span>
      </div>
      <span class="faint tiny nowrap">
        {{ messages.artifacts.summary(visibleArtifacts.length, statusCount(visibleArtifacts, "Safe"), statusCount(visibleArtifacts, "Review"), statusCount(visibleArtifacts, "Unsafe")) }}
      </span>
    </div>

    <div v-for="group in groups" :key="group.key" class="card">
      <div class="group-head">
        <button class="btn btn--ghost" @click="toggleCollapsed(group.key)">
          <span>{{ isCollapsed(group.key) ? "▸" : "▾" }}</span>
          <span class="group-head__path">{{ group.label }}</span>
          <span class="faint tiny">{{ messages.artifacts.groupMeta(group.items.length, formatBytes(group.bytes)) }}</span>
        </button>
        <span style="flex: 1"></span>
        <label class="row-wrap tiny faint" style="gap: 6px; padding-right: 12px">
          <input
            class="check"
            type="checkbox"
            :checked="groupAllSelected(group.selectable)"
            :disabled="group.selectable.length === 0"
            @change="toggleGroupArtifacts(group.selectable, !groupAllSelected(group.selectable))"
          />
          {{ messages.artifacts.selectAllSafe + " " }}
        </label>
      </div>

      <div v-if="!isCollapsed(group.key)" class="card__body card__body--flush">
        <table class="table">
          <tbody>
            <template v-for="item in group.items" :key="item.path">
              <tr :class="{ 'is-bundled': bundledOwner(item) !== null }">
                <td class="cell-check">
                  <input
                    class="check"
                    type="checkbox"
                    :checked="isSelectedArtifact(item.path)"
                    :disabled="item.safety.status === 'Unsafe' || bundledOwner(item) !== null"
                    :title="
                      item.safety.status === 'Unsafe'
                        ? messages.common.unsafeTooltip
                        : bundledOwner(item) !== null
                          ? messages.artifacts.bundledTitle
                          : ''
                    "
                    @change="toggleArtifact(item.path)"
                  />
                </td>
                <td>
                  <div class="row-wrap">
                    <span class="chip">{{ item.kind }}</span>
                    <span class="mono">{{ shortenPath(item.path, root) }}</span>
                    <span v-if="bundledOwner(item) !== null" class="badge badge--neutral">{{ messages.artifacts.bundledBadge }}</span>
                  </div>
                  <div class="tiny faint">
                    {{ item.ecosystem }} · {{ item.file_count > 0 ? messages.units.fileCount(formatCount(item.file_count)) : messages.artifacts.fileCountUnknown }}
                    <span v-if="item.is_symlink">{{ messages.artifacts.symlink(item.symlink_target ?? "") }}</span>
                  </div>
                </td>
                <td class="nowrap tiny">{{ gitState(item) }}</td>
                <td class="nowrap tiny">{{ formatRelTime(item.last_updated_at) }}</td>
                <td class="cell-size numeric">{{ sizeText(item) }}</td>
                <td class="nowrap"><SafetyBadge :status="item.safety.status" /></td>
                <td style="max-width: 260px"><ReasonList :reasons="item.safety.reasons" :limit="1" /></td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
    </div>

    <div v-if="groups.length === 0" class="card">
      <div class="empty">{{ messages.artifacts.empty }}</div>
    </div>

    <div class="actionbar">
      <span class="muted">
        {{ " " + messages.artifacts.selectedSummary(selectedSummary.artifacts, selectedSummary.worktrees) }} <strong>{{ formatBytes(selectedSummary.bytes) }}</strong>
      </span>
      <span style="flex: 1"></span>
      <button class="btn" @click="goTo('worktrees')">{{ messages.artifacts.backToWorktrees }}</button>
      <button
        class="btn btn--primary"
        :disabled="selectedSummary.worktrees + selectedSummary.artifacts === 0"
        @click="goTo('plan')"
      >
        {{ " " + messages.artifacts.generatePlan + " " }}
      </button>
    </div>
  </div>
</template>

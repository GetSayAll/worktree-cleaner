<script setup lang="ts">
import { computed } from "vue";
import {
  cancelWorkspaceScan,
  goTo,
  openWorkspaceScan,
  scanAllWorkspaceRepositories,
  startScan,
  workspaceScanEntries,
  workspaceScanState,
  workspaceScanSummary,
  workspaceSnapshot
} from "../state";
import { homePath, warningText } from "../format";
import { messages } from "../i18n";
import { isDesktop } from "../platform";
import type { WorkspaceRepository } from "../types";

const workspace = computed(() => workspaceSnapshot.value);
const repositories = computed(() =>
  [...(workspace.value?.repositories ?? [])].sort((a, b) => {
    if (a.worktree_count === null && b.worktree_count === null) return a.path.localeCompare(b.path);
    if (a.worktree_count === null) return 1;
    if (b.worktree_count === null) return -1;
    return b.worktree_count - a.worktree_count || a.path.localeCompare(b.path);
  })
);
const batch = workspaceScanState;
const summary = workspaceScanSummary;
const desktop = isDesktop();

function entryFor(path: string) {
  return workspaceScanEntries.value[path] ?? null;
}

function rowStatus(path: string): "pending" | "scanning" | "done" | "failed" {
  return entryFor(path)?.status ?? "pending";
}

function worktreeCount(repository: WorkspaceRepository): number | null {
  const entry = entryFor(repository.path);
  if (entry?.status === "done" && entry.snapshot) return entry.snapshot.worktrees.length;
  return repository.worktree_count;
}

function summaryFor(path: string): string | null {
  const entry = entryFor(path);
  if (!entry || entry.status !== "done" || !entry.snapshot) return null;
  const records = [...entry.snapshot.worktrees, ...entry.snapshot.artifacts];
  const count = (status: "Safe" | "Review" | "Unsafe") =>
    records.filter((record) => record.safety.status === status).length;
  return messages.workspace.scanSummary(
    entry.snapshot.worktrees.length,
    entry.snapshot.artifacts.length,
    count("Safe"),
    count("Review"),
    count("Unsafe")
  );
}

function openRepository(path: string) {
  const entry = entryFor(path);
  if (entry?.status === "done") {
    openWorkspaceScan(path);
    return;
  }
  void startScan(path, workspace.value?.root, false);
}

function scanAll() {
  void scanAllWorkspaceRepositories();
}
</script>

<template>
  <div class="stack page--wide">
    <div class="spread">
      <div>
        <h1>{{ messages.workspace.title }}</h1>
        <p class="faint tiny mono">{{ homePath(workspace?.root ?? "") }}</p>
      </div>
      <button class="btn" @click="goTo('welcome')">{{ messages.workspace.chooseAnother }}</button>
    </div>

    <p class="faint tiny">{{ messages.workspace.lede }}</p>

    <div v-for="warning in workspace?.warnings || []" :key="warning" class="banner banner--warn">
      <span>⚠</span>
      <span>{{ warningText(warning) }}</span>
    </div>

    <div v-if="summary.done > 0" class="notice">
      <span>{{ messages.workspace.batchSummary(summary.done, summary.worktrees, summary.artifacts) }}</span>
      <span class="badge badge--safe">{{ messages.status.short.Safe }} {{ summary.Safe }}</span>
      <span class="badge badge--review">{{ messages.status.short.Review }} {{ summary.Review }}</span>
      <span class="badge badge--unsafe">{{ messages.status.short.Unsafe }} {{ summary.Unsafe }}</span>
      <span v-if="summary.failed > 0" class="faint tiny">{{ messages.workspace.batchFailed(summary.failed) }}</span>
    </div>

    <div class="card">
      <div class="card__head">
        <div>
          <h2>{{ messages.workspace.repositoriesTitle(repositories.length) }}</h2>
          <span class="faint tiny">{{ messages.workspace.batchHint }}</span>
        </div>
        <div v-if="desktop" class="row-wrap">
          <span v-if="batch.running" class="faint tiny">
            {{ messages.workspace.scanningAll(batch.completed, batch.total) }}
          </span>
          <button v-if="batch.running" class="btn" @click="cancelWorkspaceScan">
            {{ messages.workspace.cancelScanAll }}
          </button>
          <button
            v-else
            class="btn btn--primary"
            :disabled="repositories.length === 0"
            @click="scanAll"
          >
            {{ messages.workspace.scanAll(repositories.length) }}
          </button>
        </div>
      </div>
      <div class="card__body card__body--flush">
        <div v-for="repository in repositories" :key="repository.path" class="item-row item-row--plain">
          <div class="item-row__main">
            <div class="item-row__head">
              <span class="item-row__name">{{ repository.name }}</span>
              <span class="chip">{{ messages.workspace.worktrees(worktreeCount(repository)) }}</span>
            </div>
            <div class="item-row__meta">
              <span class="item-row__where mono" :title="repository.path">{{ homePath(repository.path) }}</span>
            </div>
            <div class="item-row__foot">
              <div class="item-row__reasons">
                <div class="reasons">
                  <div v-if="rowStatus(repository.path) === 'pending'" class="reason reason--neutral">
                    <span class="reason__mark">·</span>
                    <span class="reason__text">{{ messages.workspace.rowEvidence }}</span>
                  </div>
                  <div v-else-if="rowStatus(repository.path) === 'scanning'" class="reason reason--neutral">
                    <span class="reason__mark">·</span>
                    <span class="reason__text">{{ messages.workspace.scanningRepository(repository.name) }}</span>
                  </div>
                  <div v-else-if="rowStatus(repository.path) === 'done'" class="reason reason--neutral">
                    <span class="reason__mark">·</span>
                    <span class="reason__text">{{ summaryFor(repository.path) }}</span>
                  </div>
                  <div v-else class="reason reason--unsafe">
                    <span class="reason__mark">!</span>
                    <span class="reason__text">{{ messages.workspace.scanFailed }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
          <div class="item-row__figures">
            <button
              v-if="rowStatus(repository.path) === 'done'"
              class="btn btn--primary"
              :disabled="batch.running"
              @click="openWorkspaceScan(repository.path)"
            >
              {{ messages.workspace.openScan }}
            </button>
            <button
              v-else
              class="btn btn--primary"
              :disabled="batch.running || rowStatus(repository.path) === 'scanning'"
              @click="openRepository(repository.path)"
            >
              {{ rowStatus(repository.path) === "failed" ? messages.workspace.retry : messages.workspace.scan }}
            </button>
          </div>
        </div>
        <div v-if="repositories.length === 0" class="empty">
          {{ messages.workspace.empty }}
        </div>
      </div>
    </div>
  </div>
</template>

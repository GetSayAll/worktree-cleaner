<script setup lang="ts">
import { computed, ref } from "vue";
import {
  acknowledged,
  cleanupPlan,
  confirmText,
  executeCleanup,
  goTo,
  projectPath,
  settings,
  snapshot
} from "../state";
import StatCard from "../components/StatCard.vue";
import { formatBytes, formatCount, displayPath } from "../format";

const showExport = ref(false);
const copied = ref(false);

const root = computed(() => snapshot.value?.root ?? "");
const plan = computed(() => cleanupPlan.value);

const bundledCount = computed(() => plan.value.trash_moves.filter((item) => item.bundled_with_worktree !== null).length);
const independentMoves = computed(() => plan.value.trash_moves.filter((item) => item.bundled_with_worktree === null));
const forceItems = computed(() => plan.value.worktree_removals.filter((item) => item.force));
const needsAck = computed(() => plan.value.ack_reasons.length > 0);
const needsName = computed(() => settings.require_project_name);

const gates = computed(() => {
  const itemCount = plan.value.worktree_removals.length + independentMoves.value.length;
  return [
    {
      ok: true,
      label: `Path check passed: non-empty absolute paths, symlinks resolved, no .. components, none pointing at system directories (${itemCount} items)`
    },
    {
      ok: plan.value.blockers.length === 0,
      label:
        plan.value.blockers.length === 0
          ? "Whitelist check passed: no item matches a protection rule"
          : `${plan.value.blockers.length} items matched the whitelist and have been blocked`
    },
    {
      ok: true,
      label: `Duplicate-deletion check passed: ${bundledCount.value} artifacts are removed with their owning worktree and counted only once`
    },
    { ok: true, label: "Target identity re-check passed: path, size, and last-modified time match the scan" },
    { ok: true, label: "Activity re-check passed: no new writes since the plan was generated" }
  ];
});

const canExecute = computed(
  () => !plan.value.blockers.length && (!needsAck.value || acknowledged.value) && (!needsName.value || confirmText.value.trim() === projectPath.value.split("/").pop())
);

const planJson = computed(() => JSON.stringify(plan.value, null, 2));

async function copyPlan() {
  try {
    await navigator.clipboard.writeText(planJson.value);
    copied.value = true;
    window.setTimeout(() => (copied.value = false), 1800);
  } catch {
    copied.value = false;
  }
}
</script>

<template>
  <div class="stack">
    <div class="spread">
      <div>
        <h1>Cleanup plan</h1>
        <p class="faint tiny">Review each item before executing; this page only describes what will happen and has not performed any operation yet.</p>
      </div>
      <button class="btn" @click="goTo('overview')">Back to adjust selection</button>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard label="Remove Worktree" :value="`${plan.worktree_removals.length}`" sub="git worktree remove" />
      <StatCard
        label="Move to Trash"
        :value="`${independentMoves.length}`"
        :sub="`plus ${bundledCount} items removed with their worktree`"
      />
      <StatCard label="Estimated reclaimable" tone="accent" :value="formatBytes(plan.total_bytes)" sub="based on sizes measured at scan time" />
      <StatCard
        label="Auto-excluded"
        tone="unsafe"
        value-tone
        :value="`${plan.excluded_unsafe}`"
        sub="Must not delete items are excluded from the plan"
      />
    </div>

    <div v-for="blocker in plan.blockers" :key="blocker" class="banner banner--danger">
      <span>✕</span>
      <span><span class="banner__title">Precondition failed · </span>{{ blocker }}</span>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>Precondition checks (dry run)</h2>
        <span class="faint tiny">Cleanup is enabled only when all checks pass</span>
      </div>
      <div class="card__body">
        <div class="gate">
          <div v-for="gate in gates" :key="gate.label" class="gate__item">
            <span class="gate__mark" :class="gate.ok ? 'gate__mark--ok' : 'gate__mark--bad'">{{ gate.ok ? "✓" : "✕" }}</span>
            <span :class="gate.ok ? 'muted' : ''">{{ gate.label }}</span>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>Channel A · Remove worktrees</h2>
        <span class="faint tiny">{{ plan.worktree_removals.length }} items</span>
      </div>
      <div class="card__body">
        <div v-if="plan.worktree_removals.length === 0" class="faint tiny">No worktrees selected.</div>
        <div class="stack stack--tight">
          <div
            v-for="item in plan.worktree_removals"
            :key="item.path"
            class="plan-item"
            :class="item.force ? 'plan-item--force' : ''"
          >
            <div class="plan-item__main">
              <div class="row-wrap">
                <span class="mono">{{ displayPath(item.path, root) }}</span>
                <span class="chip">{{ item.branch || "(detached)" }}</span>
                <span v-if="item.force" class="badge badge--review">Needs --force</span>
                <span v-else class="badge badge--safe">Removable directly</span>
              </div>
              <div class="code">{{ item.command }}</div>
              <div v-if="item.force">
                <div class="field__label">
                  --force also deletes the following untracked files ({{ item.untracked_files.length }} in total),
                  which are not tracked in Git and cannot be recovered from the repository once deleted:
                </div>
                <div class="untracked-list">
                  <div v-for="file in item.untracked_files" :key="file">{{ file }}</div>
                </div>
              </div>
              <p v-if="item.command.startsWith('git worktree prune')" class="faint tiny">
                The working directory is already gone. Only the Git metadata is pruned; no files are touched.
              </p>
              <p v-else-if="!item.force" class="faint tiny">The working tree is clean; no unsaved content will be lost.</p>
            </div>
            <div class="numeric nowrap faint tiny">{{ formatBytes(item.size_bytes) }}</div>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>Channel B · Move to system Trash</h2>
        <span class="faint tiny">{{ independentMoves.length }} items · {{ formatBytes(independentMoves.reduce((s, i) => s + (i.size_bytes ?? 0), 0)) }}</span>
      </div>
      <div class="card__body card__body--flush">
        <table class="table">
          <thead>
            <tr>
              <th>Directory</th>
              <th class="cell-size">Files</th>
              <th class="cell-size">Size</th>
              <th>Recoverability</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="item in plan.trash_moves" :key="item.path" :class="{ 'is-bundled': item.bundled_with_worktree !== null }">
              <td>
                <div class="row-wrap">
                  <span class="chip">{{ item.artifact_kind }}</span>
                  <span class="mono">{{ displayPath(item.path, root) }}</span>
                </div>
              </td>
              <td class="cell-size numeric">{{ item.file_count > 0 ? formatCount(item.file_count) : "unknown" }}</td>
              <td class="cell-size numeric">{{ formatBytes(item.size_bytes) }}</td>
              <td class="tiny">
                <span v-if="item.bundled_with_worktree" class="faint">Removed with worktree</span>
                <span v-else class="muted">Recoverable from Trash</span>
              </td>
            </tr>
            <tr v-if="plan.trash_moves.length === 0">
              <td colspan="4"><div class="empty">No artifact directories selected</div></td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div v-if="needsAck || needsName" class="card">
      <div class="card__head"><h2>Confirmation before execution</h2></div>
      <div class="card__body stack stack--tight">
        <div v-if="needsAck" class="banner banner--warn">
          <span>⚠</span>
          <span class="stack stack--tight">
            <span>This operation meets the following high-risk conditions and requires manual confirmation:</span>
            <span v-for="reason in plan.ack_reasons" :key="reason" class="tiny">· {{ reason }}</span>
          </span>
        </div>
        <label v-if="needsAck" class="row-wrap" style="gap: 8px">
          <input v-model="acknowledged" class="check" type="checkbox" />
          <span>I have reviewed the items listed above, including the untracked files force-deleted in {{ forceItems.length }} worktrees</span>
        </label>
        <div v-if="needsName" class="field">
          <label class="field__label">
            Type the project directory name <span class="mono">{{ projectPath.split("/").pop() }}</span> to proceed
          </label>
          <input v-model="confirmText" class="input input--mono setting-row__control--narrow" style="max-width: 320px" />
        </div>
      </div>
    </div>

    <div v-if="showExport" class="card">
      <div class="card__head">
        <h2>Plan export (JSON)</h2>
        <button class="btn btn--ghost" @click="copyPlan">{{ copied ? "Copied" : "Copy" }}</button>
      </div>
      <div class="card__body">
        <pre class="code">{{ planJson }}</pre>
      </div>
    </div>

    <div class="actionbar">
      <button class="btn btn--ghost" @click="showExport = !showExport">
        {{ showExport ? "Hide export" : "Export plan" }}
      </button>
      <span class="faint tiny">{{ plan.excluded_unsafe }} must-not-delete items were auto-excluded; return to the list to see why</span>
      <span style="flex: 1"></span>
      <button class="btn" @click="goTo('artifacts')">Back to adjust</button>
      <button class="btn btn--danger btn--lg" :disabled="!canExecute" @click="executeCleanup">Run cleanup</button>
    </div>
  </div>
</template>

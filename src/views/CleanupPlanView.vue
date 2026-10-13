<script setup lang="ts">
import { computed, ref } from "vue";
import {
  acknowledged,
  cleanupPlan,
  confirmText,
  executeCleanup,
  executeError,
  executing,
  goTo,
  projectPath,
  selectedWorktrees,
  settings,
  snapshot
} from "../state";
import { isDesktop } from "../platform";

const desktop = isDesktop();
import StatCard from "../components/StatCard.vue";
import { basename, displayPath, formatBytes, formatCount, isAbsolutePath, normalizePathKey } from "../format";
import { messages } from "../i18n";

const showExport = ref(false);
const copied = ref(false);

const root = computed(() => snapshot.value?.root ?? "");
const plan = computed(() => cleanupPlan.value);

const bundledCount = computed(() => plan.value.trash_moves.filter((item) => item.bundled_with_worktree !== null).length);
const independentMoves = computed(() => plan.value.trash_moves.filter((item) => item.bundled_with_worktree === null));
// Independent trash moves summed on their own: an unmeasurable item adds nothing
// to the total, so the header figure is a lower bound and the count of
// unmeasurable items must travel with it.
const trashBytes = computed(() => ({
  bytes: independentMoves.value.reduce((sum, item) => sum + (item.size_bytes ?? 0), 0),
  unknown: independentMoves.value.filter((item) => item.size_bytes === null).length
}));
const forceItems = computed(() => plan.value.worktree_removals.filter((item) => item.force));
const needsAck = computed(() => plan.value.ack_reasons.length > 0);
const needsName = computed(() => settings.require_project_name);
const projectName = computed(() => basename(projectPath.value));

/** Paths the plan is about to touch, used by the path precondition check. */
const itemPaths = computed(() => [
  ...plan.value.worktree_removals.map((item) => item.path),
  ...independentMoves.value.map((item) => item.path)
]);

function isVerifiedPath(path: string): boolean {
  if (!path || path.includes("\0")) return false;
  if (!isAbsolutePath(path)) return false;
  return !normalizePathKey(path).split("/").some((segment) => segment === "..");
}

const pathsVerified = computed(() => itemPaths.value.every(isVerifiedPath));

/**
 * The plan already marks every artifact owned by a selected worktree as bundled;
 * this re-derives the marking from the snapshot so the gate reflects the data
 * rather than a constant.
 */
const dedupOk = computed(() =>
  plan.value.trash_moves.every((move) => {
    const owner = snapshot.value?.artifacts.find((item) => item.path === move.path)?.worktree_path ?? null;
    const bundled = owner !== null && selectedWorktrees.value.includes(owner);
    return bundled === (move.bundled_with_worktree !== null);
  })
);

const gates = computed(() => {
  const itemCount = plan.value.worktree_removals.length + independentMoves.value.length;
  return [
    {
      ok: pathsVerified.value,
      pending: false,
      label: messages.plan.checks.pathCheck(itemCount)
    },
    {
      ok: plan.value.blockers.length === 0,
      pending: false,
      label:
        plan.value.blockers.length === 0
          ? messages.plan.checks.whitelistPassed
          : messages.plan.checks.whitelistBlocked(plan.value.blockers.length)
    },
    {
      ok: dedupOk.value,
      pending: false,
      label: messages.plan.checks.duplicateCheck(bundledCount.value)
    },
    // Target identity and activity cannot be re-checked before execution (D11:
    // the re-check happens between plan generation and execution); say so instead
    // of claiming a check that never ran.
    { ok: false, pending: true, label: messages.plan.checks.identityNotRun },
    { ok: false, pending: true, label: messages.plan.checks.activityNotRun }
  ];
});

const hasPlanItems = computed(() => plan.value.worktree_removals.length + plan.value.trash_moves.length > 0);

const canExecute = computed(
  () => hasPlanItems.value && !plan.value.blockers.length && (!needsAck.value || acknowledged.value) && (!needsName.value || confirmText.value.trim() === projectName.value)
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
        <h1>{{ messages.plan.title }}</h1>
        <p class="faint tiny">{{ messages.plan.lede }}</p>
      </div>
      <button class="btn" @click="goTo('overview')">{{ messages.plan.backToAdjustSelection }}</button>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard :label="messages.plan.stats.removeWorktree" :value="`${plan.worktree_removals.length}`" :sub="messages.plan.stats.removeWorktreeSub" />
      <StatCard
        :label="messages.plan.stats.moveToTrash"
        :value="`${independentMoves.length}`"
        :sub="messages.plan.stats.moveToTrashSub(bundledCount)"
      />
      <StatCard
        :label="messages.plan.stats.reclaimable"
        tone="accent"
        :value="formatBytes(plan.total_bytes, { approximate: plan.total_bytes_unknown > 0 })"
        :sub="plan.total_bytes_unknown > 0 ? messages.plan.stats.reclaimableSubUnknown(plan.total_bytes_unknown) : messages.plan.stats.reclaimableSub"
      />
      <StatCard
        :label="messages.plan.stats.autoExcluded"
        tone="unsafe"
        value-tone
        :value="`${plan.excluded_unsafe}`"
        :sub="messages.plan.stats.autoExcludedSub"
      />
    </div>

    <div v-for="blocker in plan.blockers" :key="blocker" class="banner banner--danger">
      <span>✕</span>
      <span><span class="banner__title">{{ messages.plan.blockerTitle }}</span>{{ blocker }}</span>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.plan.checks.title }}</h2>
        <span class="faint tiny">{{ messages.plan.checks.note }}</span>
      </div>
      <div class="card__body">
        <div class="gate">
          <div v-for="gate in gates" :key="gate.label" class="gate__item">
            <span class="gate__mark" :class="gate.pending ? '' : gate.ok ? 'gate__mark--ok' : 'gate__mark--bad'">{{ gate.pending ? "–" : gate.ok ? "✓" : "✕" }}</span>
            <span :class="gate.pending ? 'faint' : gate.ok ? 'muted' : ''">{{ gate.label }}</span>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.plan.channelA }}</h2>
        <span class="faint tiny">{{ messages.plan.items(plan.worktree_removals.length) }}</span>
      </div>
      <div class="card__body">
        <div v-if="plan.worktree_removals.length === 0" class="faint tiny">{{ messages.plan.noWorktrees }}</div>
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
                <span class="chip">{{ item.branch || messages.common.detached }}</span>
                <span v-if="item.force" class="badge badge--review">{{ messages.plan.needForce }}</span>
                <span v-else class="badge badge--safe">{{ messages.plan.removable }}</span>
              </div>
              <div class="code">{{ item.command }}</div>
              <div v-if="item.force">
                <div class="field__label">
                  {{ " " + messages.plan.forceNote(item.untracked_files.length) + " " }}
                </div>
                <div class="untracked-list">
                  <div v-for="file in item.untracked_files" :key="file">{{ file }}</div>
                </div>
              </div>
              <p v-if="item.command.startsWith('git worktree prune')" class="faint tiny">
                {{ " " + messages.plan.prunableNote + " " }}
              </p>
              <p v-else-if="!item.force" class="faint tiny">{{ messages.plan.cleanNote }}</p>
            </div>
            <div class="numeric nowrap faint tiny">{{ formatBytes(item.size_bytes) }}</div>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.plan.channelB }}</h2>
        <span class="faint tiny">{{ messages.plan.trashHeader(independentMoves.length, formatBytes(trashBytes.bytes, { approximate: trashBytes.unknown > 0 })) }}</span>
      </div>
      <div class="card__body card__body--flush">
        <table class="table">
          <thead>
            <tr>
              <th>{{ messages.plan.columns.directory }}</th>
              <th class="cell-size">{{ messages.plan.columns.files }}</th>
              <th class="cell-size">{{ messages.plan.columns.size }}</th>
              <th>{{ messages.plan.columns.recoverability }}</th>
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
              <td class="cell-size numeric">{{ item.file_count > 0 ? formatCount(item.file_count) : messages.common.unknown }}</td>
              <td class="cell-size numeric">{{ formatBytes(item.size_bytes) }}</td>
              <td class="tiny">
                <span v-if="item.bundled_with_worktree" class="faint">{{ messages.plan.removedWithWorktree }}</span>
                <span v-else class="muted">{{ messages.plan.recoverable }}</span>
              </td>
            </tr>
            <tr v-if="plan.trash_moves.length === 0">
              <td colspan="4"><div class="empty">{{ messages.plan.noArtifacts }}</div></td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div v-if="needsAck || needsName" class="card">
      <div class="card__head"><h2>{{ messages.plan.confirmationTitle }}</h2></div>
      <div class="card__body stack stack--tight">
        <div v-if="needsAck" class="banner banner--warn">
          <span>⚠</span>
          <span class="stack stack--tight">
            <span>{{ messages.plan.confirmationIntro }}</span>
            <span v-for="reason in plan.ack_reasons" :key="reason" class="tiny">· {{ reason }}</span>
          </span>
        </div>
        <label v-if="needsAck" class="row-wrap" style="gap: 8px">
          <input v-model="acknowledged" class="check" type="checkbox" />
          <span>{{ messages.plan.ackConfirm(forceItems.length) }}</span>
        </label>
        <div v-if="needsName" class="field">
          <label class="field__label">
            {{ " " + messages.plan.typeNameBefore }} <span class="mono">{{ projectName }}</span> {{ messages.plan.typeNameAfter + " " }}
          </label>
          <input v-model="confirmText" class="input input--mono setting-row__control--narrow" style="max-width: 320px" />
        </div>
      </div>
    </div>

    <div v-if="showExport" class="card">
      <div class="card__head">
        <h2>{{ messages.plan.exportTitle }}</h2>
        <button class="btn btn--ghost" @click="copyPlan">{{ copied ? messages.common.copied : messages.plan.copy }}</button>
      </div>
      <div class="card__body">
        <pre class="code">{{ planJson }}</pre>
      </div>
    </div>

    <div class="actionbar">
      <button class="btn btn--ghost" @click="showExport = !showExport">
        {{ showExport ? messages.plan.hideExport : messages.plan.exportPlan }}
      </button>
      <span class="faint tiny">{{ messages.plan.excludedNote(plan.excluded_unsafe) }}</span>
      <span style="flex: 1"></span>
      <button class="btn" @click="goTo('artifacts')">{{ messages.plan.backToAdjust }}</button>
      <button
        class="btn btn--danger btn--lg"
        data-action="run-cleanup"
        :disabled="!canExecute || executing || !desktop"
        @click="executeCleanup"
      >
        {{ executing ? messages.plan.running : messages.plan.runCleanup }}
      </button>
    </div>

    <p v-if="!desktop" class="notice">{{ messages.preview.notice }}</p>
    <p v-if="executeError" class="notice notice--unsafe">{{ executeError }}</p>
  </div>
</template>

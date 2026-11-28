<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import {
  acknowledged,
  buildCachePlan,
  buildCacheScanning,
  cleanupPlan,
  confirmText,
  executeCleanup,
  executeErrorMessage,
  executing,
  executionProgress,
  goTo,
  projectPath,
  recordPlanExport,
  selectedBuildCaches,
  selectedWorktrees,
  settings,
  snapshot,
  toggleBuildCache
} from "../state";
import { isDesktop } from "../platform";
import type { WorktreeRemovalItem } from "../types";
import { reviewLane, type ReviewLane } from "../worktreeGroups";

const desktop = isDesktop();
import StatCard from "../components/StatCard.vue";
import { basename, displayPath, formatBytes, formatCount, isAbsolutePath, normalizePathKey } from "../format";
import { messages } from "../i18n";

const showExport = ref(false);
const copied = ref(false);

const root = computed(() => snapshot.value?.root ?? "");
const plan = computed(() => cleanupPlan.value);
const cachePlan = computed(() => buildCachePlan.value);
const cacheMoves = computed(() => cachePlan.value.trash_moves);
const cacheBytes = computed(() =>
  cacheMoves.value.reduce((sum, item) => sum + (item.size_bytes ?? 0), 0)
);
const cacheUnknown = computed(
  () => cacheMoves.value.filter((item) => item.size_bytes === null).length
);
const allBlockers = computed(() => [...plan.value.blockers, ...cachePlan.value.blockers]);
const ackReasons = computed(() => [
  ...new Set([...plan.value.ack_reasons, ...cachePlan.value.ack_reasons])
]);
const combinedTotal = computed(() => plan.value.total_bytes + cachePlan.value.total_bytes);
const combinedUnknown = computed(
  () => plan.value.total_bytes_unknown + cachePlan.value.total_bytes_unknown
);
const excludedUnsafe = computed(
  () => plan.value.excluded_unsafe + cachePlan.value.excluded_unsafe
);

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
const permanentRemovals = computed(
  () => plan.value.worktree_removals.filter((item) => !item.command.startsWith("git worktree prune")).length
);
const worktreeStatus = computed(
  () => new Map((snapshot.value?.worktrees ?? []).map((record) => [record.path, record.safety.status]))
);
const artifactStatus = computed(
  () => new Map((snapshot.value?.artifacts ?? []).map((record) => [record.path, record.safety.status]))
);

type RemovalGroupKey = "safe" | ReviewLane;

interface RemovalGroup {
  key: RemovalGroupKey;
  items: WorktreeRemovalItem[];
}

const removalGroupOrder: RemovalGroupKey[] = ["safe", "batch", "untracked", "attention"];

/**
 * The plan is grouped with the same loss profiles as the worktree page. A group
 * header repeats the impact and the count; the rows remain available underneath,
 * so this is a way to read 50 clean removals as one decision without hiding the
 * paths the executor will touch.
 */
const removalGroups = computed<RemovalGroup[]>(() => {
  const records = new Map(
    (snapshot.value?.worktrees ?? []).map((record) => [record.path, record])
  );
  const buckets = new Map<RemovalGroupKey, WorktreeRemovalItem[]>(
    removalGroupOrder.map((key) => [key, []])
  );
  for (const item of plan.value.worktree_removals) {
    const record = records.get(item.path);
    const lane =
      record?.safety.status === "Review"
        ? reviewLane(record) ?? "attention"
        : record?.safety.status === "Safe"
          ? "safe"
          : "attention";
    buckets.get(lane)?.push(item);
  }
  return removalGroupOrder
    .map((key) => ({ key, items: buckets.get(key) ?? [] }))
    .filter((group) => group.items.length > 0);
});

const collapsedRemovalGroups = ref<RemovalGroupKey[]>(["batch"]);

function removalGroupBytes(group: RemovalGroup): string {
  const bytes = group.items.reduce((sum, item) => sum + (item.size_bytes ?? 0), 0);
  const unknown = group.items.filter((item) => item.size_bytes === null).length;
  return formatBytes(bytes, { approximate: unknown > 0 });
}

function removalGroupTitle(key: RemovalGroupKey): string {
  switch (key) {
    case "safe":
      return messages.plan.removalGroups.safe;
    case "batch":
      return messages.plan.removalGroups.batch;
    case "untracked":
      return messages.plan.removalGroups.untracked;
    case "attention":
      return messages.plan.removalGroups.attention;
  }
}

function removalGroupImpact(key: RemovalGroupKey): string {
  return messages.plan.removalGroups.impact[key];
}

function isRemovalGroupCollapsed(group: RemovalGroup): boolean {
  // Short groups do not need a second click; only a long group starts folded.
  return group.items.length > 3 && collapsedRemovalGroups.value.includes(group.key);
}

function toggleRemovalGroup(group: RemovalGroup) {
  collapsedRemovalGroups.value = collapsedRemovalGroups.value.includes(group.key)
    ? collapsedRemovalGroups.value.filter((key) => key !== group.key)
    : [...collapsedRemovalGroups.value, group.key];
}

type ProgressState = "" | "removed" | "kept";

/** The live result for one row, while the executor is still working. */
function progressState(path: string, prune = false): ProgressState {
  const entry = executionProgress.value.find((item) =>
    prune ? item.action === "prune" : item.target === path
  );
  if (!entry) return "";
  return entry.result === "ok" ? "removed" : "kept";
}

// Follow the item that just reported. Without this the animation can run on a
// row far outside the viewport, which tells the reader that something happened
// but not what. `scrollIntoView` walks up to the pane that actually scrolls.
watch(
  () => executionProgress.value.length,
  async () => {
    const entry = executionProgress.value.at(-1);
    if (!entry) return;
    await nextTick();
    const row =
      entry.action === "prune"
        ? document.querySelector("[data-plan-prune]")
        : Array.from(document.querySelectorAll("[data-plan-target]")).find(
            (element) => element.getAttribute("data-plan-target") === entry.target
          );
    row?.scrollIntoView({ block: "center" });
  }
);
const needsAck = computed(() => ackReasons.value.length > 0);
const needsName = computed(() => settings.require_project_name);
const projectName = computed(() => basename(projectPath.value));

/** Paths the plan is about to touch, used by the path precondition check. */
const itemPaths = computed(() => [
  ...plan.value.worktree_removals.map((item) => item.path),
  ...independentMoves.value.map((item) => item.path),
  ...cacheMoves.value.map((item) => item.path)
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
  const itemCount =
    plan.value.worktree_removals.length + independentMoves.value.length + cacheMoves.value.length;
  return [
    {
      ok: pathsVerified.value,
      pending: false,
      label: messages.plan.checks.pathCheck(itemCount)
    },
    {
      ok: allBlockers.value.length === 0,
      pending: false,
      label:
        allBlockers.value.length === 0
          ? messages.plan.checks.whitelistPassed
          : messages.plan.checks.whitelistBlocked(allBlockers.value.length)
    },
    {
      ok: dedupOk.value,
      pending: false,
      label: messages.plan.checks.duplicateCheck(bundledCount.value)
    },
    // Target identity and activity are re-checked between plan generation and
    // execution (D11), never before, so these two stay pending here. Each label
    // says what the re-check will look at; the dash in the list and `checks.note`
    // already carry the state and the timing.
    { ok: false, pending: true, label: messages.plan.checks.identityNotRun },
    { ok: false, pending: true, label: messages.plan.checks.activityNotRun }
  ];
});

const hasPlanItems = computed(
  () =>
    plan.value.worktree_removals.length +
      plan.value.trash_moves.length +
      cacheMoves.value.length >
    0
);

const canExecute = computed(
  () =>
    hasPlanItems.value &&
    !buildCacheScanning.value &&
    allBlockers.value.length === 0 &&
    (!needsAck.value || acknowledged.value) &&
    (!needsName.value || confirmText.value.trim() === projectName.value)
);

const planJson = computed(() =>
  JSON.stringify(
    cacheMoves.value.length > 0
      ? { project: plan.value, build_caches: cachePlan.value }
      : plan.value,
    null,
    2
  )
);

async function copyPlan() {
  try {
    await navigator.clipboard.writeText(planJson.value);
    copied.value = true;
    // An exported plan is a plan a run can be started from later, and the log is
    // the only place that would otherwise remember one was taken.
    recordPlanExport(
      plan.value.worktree_removals.length + independentMoves.value.length + cacheMoves.value.length
    );
    window.setTimeout(() => (copied.value = false), 1800);
  } catch {
    copied.value = false;
  }
}
</script>

<template>
  <div class="stack page--wide">
    <div class="spread">
      <div>
        <h1>{{ messages.plan.title }}</h1>
        <p class="faint tiny">{{ messages.plan.lede }}</p>
      </div>
      <button class="btn" :disabled="executing" @click="goTo('overview')">{{ messages.plan.backToAdjustSelection }}</button>
    </div>

    <p class="rules-summary"><strong>{{ messages.rules.shortLabel }}</strong>{{ messages.rules.planSummary }}</p>

    <div v-if="buildCacheScanning" class="banner banner--warn">
      <span>⚠</span>
      <span>{{ messages.plan.buildCacheScanning }}</span>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard :label="messages.plan.stats.removeWorktree" :value="`${plan.worktree_removals.length}`" :sub="messages.plan.stats.removeWorktreeSub" />
      <StatCard
        :label="messages.plan.stats.moveToTrash"
        :value="`${independentMoves.length + cacheMoves.length}`"
        :sub="messages.plan.stats.moveToTrashSub(bundledCount)"
      />
      <StatCard
        :label="messages.plan.stats.reclaimable"
        tone="accent"
        :value="formatBytes(combinedTotal, { approximate: combinedUnknown > 0 })"
        :sub="combinedUnknown > 0 ? messages.plan.stats.reclaimableSubUnknown(combinedUnknown) : messages.plan.stats.reclaimableSub"
      />
      <StatCard
        :label="messages.plan.stats.autoExcluded"
        tone="unsafe"
        value-tone
        :value="`${excludedUnsafe}`"
        :sub="messages.plan.stats.autoExcludedSub"
      />
    </div>

    <div v-for="blocker in allBlockers" :key="blocker" class="banner banner--danger">
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

    <div v-if="needsAck" class="card">
      <div class="card__head"><h2>{{ messages.plan.confirmationTitle }}</h2></div>
      <div class="card__body">
        <div class="banner banner--warn">
          <span>⚠</span>
          <span class="stack stack--tight">
            <span>{{ messages.plan.confirmationIntro }}</span>
            <span v-for="reason in ackReasons" :key="reason" class="tiny">· {{ reason }}</span>
          </span>
        </div>
      </div>
    </div>

    <div v-if="permanentRemovals > 0" class="banner banner--warn">
      <span>⚠</span>
      <span>{{ messages.plan.permanenceNote }}</span>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.plan.channelA }}</h2>
        <span class="faint tiny">{{ messages.plan.items(plan.worktree_removals.length) }}</span>
      </div>
      <div class="card__body">
        <div v-if="plan.worktree_removals.length === 0" class="faint tiny">{{ messages.plan.noWorktrees }}</div>
        <template v-else>
          <div v-for="group in removalGroups" :key="group.key" class="plan-removal-group">
            <div class="review-group-head" :data-plan-group="group.key">
              <div class="review-group-head__body">
                <div class="row-wrap">
                  <span class="review-group-head__title">{{ removalGroupTitle(group.key) }}</span>
                  <span class="faint tiny">
                    {{ messages.plan.removalGroups.meta(group.items.length, removalGroupBytes(group)) }}
                  </span>
                </div>
                <p class="review-group-head__impact">{{ removalGroupImpact(group.key) }}</p>
              </div>
              <div v-if="group.items.length > 3" class="review-group-head__action">
                <button
                  class="btn btn--ghost"
                  :aria-expanded="!isRemovalGroupCollapsed(group)"
                  :disabled="executing"
                  @click="toggleRemovalGroup(group)"
                >
                  {{ isRemovalGroupCollapsed(group) ? messages.plan.removalGroups.showPaths(group.items.length) : messages.plan.removalGroups.hidePaths }}
                </button>
              </div>
            </div>
            <div v-if="!isRemovalGroupCollapsed(group)" class="stack stack--tight plan-removal-group__rows">
              <div
                v-for="item in group.items"
                :key="item.path"
                class="plan-item"
                :class="{
                  'plan-item--force': item.force,
                  'is-removed': progressState(item.path, item.command.startsWith('git worktree prune')) === 'removed',
                  'is-kept': progressState(item.path, item.command.startsWith('git worktree prune')) === 'kept'
                }"
                :data-plan-target="item.command.startsWith('git worktree prune') ? undefined : item.path"
                :data-plan-prune="item.command.startsWith('git worktree prune') ? 'true' : undefined"
              >
                <div class="plan-item__main">
                  <div class="row-wrap">
                    <span class="mono">{{ displayPath(item.path, root) }}</span>
                    <span class="chip">{{ item.branch || messages.common.detached }}</span>
                    <span v-if="item.force" class="badge badge--review">{{ messages.plan.needForce }}</span>
                    <span v-else-if="worktreeStatus.get(item.path) === 'Review'" class="badge badge--review">{{ messages.plan.reviewItem }}</span>
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
        </template>
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
              <th class="cell-directory">{{ messages.plan.columns.directory }}</th>
              <th class="cell-size cell-bytes">{{ messages.plan.columns.files }}</th>
              <th class="cell-size cell-bytes">{{ messages.plan.columns.size }}</th>
              <th class="cell-recoverable">{{ messages.plan.columns.recoverability }}</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="item in plan.trash_moves"
              :key="item.path"
              :class="{
                'is-bundled': item.bundled_with_worktree !== null,
                'is-removed': progressState(item.path) === 'removed',
                'is-kept': progressState(item.path) === 'kept'
              }"
              :data-plan-target="item.path"
            >
              <td class="cell-directory">
                <div class="row-wrap">
                  <span class="chip">{{ item.artifact_kind }}</span>
                  <span v-if="artifactStatus.get(item.path) === 'Review'" class="badge badge--review">{{ messages.plan.reviewItem }}</span>
                  <span class="mono">{{ displayPath(item.path, root) }}</span>
                </div>
              </td>
              <td class="cell-size cell-bytes numeric">{{ item.file_count > 0 ? formatCount(item.file_count) : messages.common.unknown }}</td>
              <td class="cell-size cell-bytes numeric">{{ formatBytes(item.size_bytes) }}</td>
              <td class="tiny cell-recoverable">
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

    <div v-if="cacheMoves.length > 0" class="card">
      <div class="card__head">
        <h2>{{ messages.plan.channelC }}</h2>
        <span class="faint tiny">
          {{ messages.plan.buildCacheHeader(cacheMoves.length, formatBytes(cacheBytes, { approximate: cacheUnknown > 0 })) }}
        </span>
      </div>
      <div class="card__body">
        <p class="faint tiny">{{ messages.plan.buildCacheNote }}</p>
        <table class="table">
          <thead>
            <tr>
              <th class="cell-directory">{{ messages.plan.columns.directory }}</th>
              <th class="cell-size cell-bytes">{{ messages.plan.columns.size }}</th>
              <th class="cell-recoverable">{{ messages.plan.columns.recoverability }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="item in cacheMoves" :key="item.path" :data-plan-target="item.path">
              <td class="cell-directory">
                <div class="row-wrap">
                  <input
                    class="check"
                    type="checkbox"
                    :checked="selectedBuildCaches.includes(item.path)"
                    :disabled="executing"
                    @change="toggleBuildCache(item.path)"
                  />
                  <span class="chip">{{ item.artifact_kind }}</span>
                  <span class="mono">{{ displayPath(item.path, root) }}</span>
                </div>
              </td>
              <td class="cell-size cell-bytes numeric">{{ formatBytes(item.size_bytes) }}</td>
              <td class="tiny cell-recoverable"><span class="muted">{{ messages.plan.recoverable }}</span></td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div v-if="showExport" class="card">
      <div class="card__head">
        <h2>{{ messages.plan.exportTitle }}</h2>
        <button class="btn btn--ghost" :disabled="executing" @click="copyPlan">{{ copied ? messages.common.copied : messages.plan.copy }}</button>
      </div>
      <div class="card__body">
        <pre class="code">{{ planJson }}</pre>
      </div>
    </div>

    <div class="actionbar actionbar--plan">
      <div v-if="hasPlanItems" class="actionbar__recheck">
        <span aria-hidden="true">⚠</span>
        <span>{{ messages.plan.checks.recheckNotice }}</span>
      </div>
      <div v-if="needsAck || needsName" class="actionbar__confirm">
        <label v-if="needsAck" class="confirm">
          <input v-model="acknowledged" class="check check--confirm" type="checkbox" :disabled="executing" />
          <span class="confirm__body">
            <span class="confirm__eyebrow">{{ acknowledged ? messages.plan.confirmed : messages.plan.confirmRequired }}</span>
            <span class="confirm__text">{{ messages.plan.ackConfirm(forceItems.length) }}</span>
            <span v-if="!acknowledged" class="confirm__hint">{{ messages.plan.confirmHint }}</span>
          </span>
        </label>
        <div v-if="needsName" class="row-wrap" style="gap: 8px">
          <span class="field__label">
            {{ " " + messages.plan.typeNameBefore }} <span class="mono">{{ projectName }}</span> {{ messages.plan.typeNameAfter + " " }}
          </span>
          <input v-model="confirmText" class="input input--mono" style="max-width: 240px" :disabled="executing" />
        </div>
        <p class="confirm__rules">{{ messages.plan.confirmRules }}</p>
      </div>
      <div class="actionbar__row">
        <div class="actionbar__side">
          <button class="btn" data-nav-back="artifacts" :disabled="executing" @click="goTo('artifacts')">
            {{ messages.plan.backToAdjust }}
          </button>
        </div>
        <span class="actionbar__summary faint tiny">{{ messages.plan.excludedNote(excludedUnsafe) }}</span>
        <div class="actionbar__side" style="margin-left: auto">
          <!-- Same height as the button beside it: two buttons of different sizes in
               one row do not read as a pair, and the taller one sets the row height so
               the shorter one floats. -->
          <button class="btn btn--lg" :disabled="executing" @click="showExport = !showExport">
            {{ showExport ? messages.plan.hideExport : messages.plan.exportPlan }}
          </button>
          <button
            class="btn btn--danger btn--lg"
            :class="executing ? 'is-busy' : ''"
            data-action="run-cleanup"
            :disabled="!canExecute || executing || !desktop"
            @click="executeCleanup"
          >
            <!-- A spinner, because a button that reads "running" and then does nothing
                 for a few seconds is indistinguishable from a hung app. The executor
                 works through the plan item by item, so the wait is real. -->
            <span v-if="executing" class="spinner" aria-hidden="true"></span>
            {{ executing ? messages.plan.running : messages.plan.runCleanup }}
          </button>
        </div>
      </div>
    </div>

    <!-- An indeterminate bar for the same reason: the core answers in one call, so
         there is no percentage to show, but there is definitely work happening and
         the page should not look inert while it does. -->
    <div v-if="executing" class="busy-strip" role="status">
      <div class="busy-strip__bar"></div>
      <span class="busy-strip__label">{{ messages.plan.runningNote }}</span>
    </div>

    <p v-if="!desktop" class="notice">{{ messages.preview.notice }}</p>
    <p v-if="executeErrorMessage" class="notice notice--unsafe">{{ executeErrorMessage }}</p>
  </div>
</template>

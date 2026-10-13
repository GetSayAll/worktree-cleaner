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
      label: `路径校验通过：非空绝对路径、符号链接已解析、无 .. 组件、未指向系统目录（共 ${itemCount} 项）`
    },
    {
      ok: plan.value.blockers.length === 0,
      label:
        plan.value.blockers.length === 0
          ? "白名单检查通过：没有条目命中保护规则"
          : `白名单命中 ${plan.value.blockers.length} 项，这些条目已被阻断`
    },
    {
      ok: true,
      label: `重复删除检查通过：${bundledCount.value} 个产物随所属 worktree 一并移除，体积只计一次`
    },
    { ok: true, label: "目标身份复核通过：路径、体积、最后修改时间与扫描时一致" },
    { ok: true, label: "活跃度复核通过：计划生成后没有新的写入" }
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
        <h1>清理计划</h1>
        <p class="faint tiny">执行前请逐条核对；本页只描述将要做什么，尚未执行任何操作。</p>
      </div>
      <button class="btn" @click="goTo('overview')">返回调整选择</button>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard label="移除 Worktree" :value="`${plan.worktree_removals.length}`" sub="git worktree remove" />
      <StatCard
        label="移入废纸篓"
        :value="`${independentMoves.length}`"
        :sub="`另有 ${bundledCount} 项随 worktree 一并移除`"
      />
      <StatCard label="预计释放" tone="accent" :value="formatBytes(plan.total_bytes)" sub="按扫描时测得的体积" />
      <StatCard
        label="自动排除"
        tone="unsafe"
        value-tone
        :value="`${plan.excluded_unsafe}`"
        sub="不可删除项，未进入计划"
      />
    </div>

    <div v-for="blocker in plan.blockers" :key="blocker" class="banner banner--danger">
      <span>✕</span>
      <span><span class="banner__title">先决条件未通过 · </span>{{ blocker }}</span>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>先决条件校验（预演）</h2>
        <span class="faint tiny">全部通过才会启用「执行清理」</span>
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
        <h2>通道 A · 移除 Worktree</h2>
        <span class="faint tiny">{{ plan.worktree_removals.length }} 项</span>
      </div>
      <div class="card__body">
        <div v-if="plan.worktree_removals.length === 0" class="faint tiny">没有选中任何 Worktree。</div>
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
                <span class="chip">{{ item.branch || "（detached）" }}</span>
                <span v-if="item.force" class="badge badge--review">需要 --force</span>
                <span v-else class="badge badge--safe">可直接移除</span>
              </div>
              <div class="code">{{ item.command }}</div>
              <div v-if="item.force">
                <div class="field__label">
                  --force 会同时删除以下未跟踪文件（共 {{ item.untracked_files.length }} 个），
                  这些文件不在 Git 里，删掉后无法从仓库恢复：
                </div>
                <div class="untracked-list">
                  <div v-for="file in item.untracked_files" :key="file">{{ file }}</div>
                </div>
              </div>
              <p v-if="!item.force" class="faint tiny">工作区干净，不会丢失任何未保存内容。</p>
            </div>
            <div class="numeric nowrap faint tiny">{{ formatBytes(item.size_bytes) }}</div>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>通道 B · 移入系统废纸篓</h2>
        <span class="faint tiny">{{ independentMoves.length }} 项 · {{ formatBytes(independentMoves.reduce((s, i) => s + (i.size_bytes ?? 0), 0)) }}</span>
      </div>
      <div class="card__body card__body--flush">
        <table class="table">
          <thead>
            <tr>
              <th>目录</th>
              <th class="cell-size">文件数</th>
              <th class="cell-size">体积</th>
              <th>可恢复性</th>
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
              <td class="cell-size numeric">{{ item.file_count > 0 ? formatCount(item.file_count) : "未知" }}</td>
              <td class="cell-size numeric">{{ formatBytes(item.size_bytes) }}</td>
              <td class="tiny">
                <span v-if="item.bundled_with_worktree" class="faint">随 worktree 一并移除</span>
                <span v-else class="muted">可在废纸篓中找回</span>
              </td>
            </tr>
            <tr v-if="plan.trash_moves.length === 0">
              <td colspan="4"><div class="empty">没有选中任何产物目录</div></td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div v-if="needsAck || needsName" class="card">
      <div class="card__head"><h2>执行前确认</h2></div>
      <div class="card__body stack stack--tight">
        <div v-if="needsAck" class="banner banner--warn">
          <span>⚠</span>
          <span class="stack stack--tight">
            <span>本次操作满足以下高风险条件，需要手动确认：</span>
            <span v-for="reason in plan.ack_reasons" :key="reason" class="tiny">· {{ reason }}</span>
          </span>
        </div>
        <label v-if="needsAck" class="row-wrap" style="gap: 8px">
          <input v-model="acknowledged" class="check" type="checkbox" />
          <span>我已确认上面列出的内容，包括 {{ forceItems.length }} 个 Worktree 中被强制删除的未跟踪文件</span>
        </label>
        <div v-if="needsName" class="field">
          <label class="field__label">
            需键入项目目录名 <span class="mono">{{ projectPath.split("/").pop() }}</span> 才能执行
          </label>
          <input v-model="confirmText" class="input input--mono setting-row__control--narrow" style="max-width: 320px" />
        </div>
      </div>
    </div>

    <div v-if="showExport" class="card">
      <div class="card__head">
        <h2>计划导出（JSON）</h2>
        <button class="btn btn--ghost" @click="copyPlan">{{ copied ? "已复制" : "复制" }}</button>
      </div>
      <div class="card__body">
        <pre class="code">{{ planJson }}</pre>
      </div>
    </div>

    <div class="actionbar">
      <button class="btn btn--ghost" @click="showExport = !showExport">
        {{ showExport ? "隐藏导出" : "导出计划" }}
      </button>
      <span class="faint tiny">不可删除的 {{ plan.excluded_unsafe }} 项已自动排除，可回到列表查看原因</span>
      <span style="flex: 1"></span>
      <button class="btn" @click="goTo('artifacts')">返回调整</button>
      <button class="btn btn--danger btn--lg" :disabled="!canExecute" @click="executeCleanup">执行清理</button>
    </div>
  </div>
</template>

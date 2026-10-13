<script setup lang="ts">
import { computed } from "vue";
import { history, projectPath, settings } from "../state";
import { homePath } from "../format";

const extraPathsText = computed({
  get: () => settings.extra_scan_paths.join("\n"),
  set: (value: string) => {
    settings.extra_scan_paths = value
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
  }
});

const whitelistText = computed({
  get: () => settings.whitelist_paths.join("\n"),
  set: (value: string) => {
    settings.whitelist_paths = value
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
  }
});

const protectedBranchesText = computed({
  get: () => settings.protected_branches.join(", "),
  set: (value: string) => {
    settings.protected_branches = value
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean);
  }
});

const logPath = "~/Library/Logs/Worktree Cleaner/operations.log";
</script>

<template>
  <div class="stack">
    <div class="spread">
      <div>
        <h1>设置</h1>
        <p class="faint tiny">当前项目：<span class="mono">{{ projectPath || "（未选择）" }}</span></p>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>扫描范围</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">额外扫描路径</div>
          <div class="setting-row__hint">每行一个；为空时使用默认路径（~/MySrc、~/dev、~/Projects、~/Code）</div>
        </div>
        <div class="setting-row__control">
          <textarea v-model="extraPathsText" class="textarea" spellcheck="false" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">最大扫描深度</div>
          <div class="setting-row__hint">相对扫描根目录，越深越慢</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.max_depth" class="input" type="number" min="1" max="12" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">单目录超时</div>
          <div class="setting-row__hint">超时后该条目按「需要确认」处理，不判为可安全删除</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.per_dir_timeout_sec" class="input" type="number" min="5" max="300" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">整轮扫描预算</div>
          <div class="setting-row__hint">预算耗尽即停止，并如实报告未扫完的部分</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.scan_budget_sec" class="input" type="number" min="60" max="3600" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>判定阈值</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">活跃天数</div>
          <div class="setting-row__hint">超过这个天数没有写入才算「安静」，否则判为需要确认</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.active_days" class="input" type="number" min="1" max="90" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">产物显示体积阈值</div>
          <div class="setting-row__hint">小于该体积的产物默认不显示，避免噪音</div>
        </div>
        <div class="setting-row__control row-wrap">
          <input v-model.number="settings.min_artifact_mb" class="slider" type="range" min="0" max="500" step="10" style="max-width: 280px" />
          <span class="numeric tiny nowrap">{{ settings.min_artifact_mb }} MB</span>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">未跟踪文件视为不安全</div>
          <div class="setting-row__hint">开启后，含未跟踪文件的 worktree 直接判为不可删除</div>
        </div>
        <div class="setting-row__control">
          <input v-model="settings.untracked_is_unsafe" class="switch" type="checkbox" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>保护规则</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">白名单路径</div>
          <div class="setting-row__hint">每行一个；命中的目录无论如何都不会进入清理计划</div>
        </div>
        <div class="setting-row__control">
          <textarea v-model="whitelistText" class="textarea" spellcheck="false" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">受保护分支</div>
          <div class="setting-row__hint">逗号分隔，支持通配符；默认分支会被自动识别并加入</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model="protectedBranchesText" class="input input--mono" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>删除方式</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">目录删除</div>
          <div class="setting-row__hint">固定为系统废纸篓；本工具不提供永久删除，也不接受绕过</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <select class="select" disabled>
            <option>系统废纸篓（macOS 废纸篓 / Windows 回收站）</option>
          </select>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Worktree 删除</div>
          <div class="setting-row__hint">统一走 git worktree remove；目录已丢失时走 git worktree prune</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <select class="select" disabled>
            <option>git worktree remove（不使用 rm -rf）</option>
          </select>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">强保护</div>
          <div class="setting-row__hint">开启后，执行清理前必须手工键入项目目录名</div>
        </div>
        <div class="setting-row__control">
          <input v-model="settings.require_project_name" class="switch" type="checkbox" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>日志与外观</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">操作日志</div>
          <div class="setting-row__hint">只写本机，不联网。当前已有 {{ history.length }} 条记录。</div>
        </div>
        <div class="setting-row__control row-wrap">
          <input class="input input--mono input--sm" :value="logPath" readonly style="max-width: 360px" />
          <button class="btn" title="原型未接入系统调用">打开目录</button>
          <button class="btn" disabled title="原型中不提供清空">清空日志</button>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">外观</div>
          <div class="setting-row__hint">跟随系统会在 macOS / Windows 明暗模式间自动切换</div>
        </div>
        <div class="setting-row__control">
          <div class="segmented">
            <button :class="settings.theme === 'system' ? 'is-active' : ''" @click="settings.theme = 'system'">跟随系统</button>
            <button :class="settings.theme === 'light' ? 'is-active' : ''" @click="settings.theme = 'light'">浅色</button>
            <button :class="settings.theme === 'dark' ? 'is-active' : ''" @click="settings.theme = 'dark'">深色</button>
          </div>
        </div>
      </div>
    </div>

    <p class="faint tiny">
      路径示例使用 <span class="mono">{{ homePath(logPath) }}</span> 这类缩写；实际写出的是完整绝对路径。
    </p>
  </div>
</template>

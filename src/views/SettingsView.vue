<script setup lang="ts">
import { computed } from "vue";
import { history, projectPath, settings } from "../state";
import { homePath } from "../format";
import { messages } from "../i18n";

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
        <h1>{{ messages.settings.title }}</h1>
        <p class="faint tiny">{{ messages.settings.currentProject }} <span class="mono">{{ projectPath || messages.common.none }}</span></p>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>{{ messages.settings.scanScopeTitle }}</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.extraScanPaths }}</div>
          <div class="setting-row__hint">{{ messages.settings.extraScanPathsHint }}</div>
        </div>
        <div class="setting-row__control">
          <textarea v-model="extraPathsText" class="textarea" spellcheck="false" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.maxDepth }}</div>
          <div class="setting-row__hint">{{ messages.settings.maxDepthHint }}</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.max_depth" class="input" type="number" min="1" max="12" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.perDirTimeout }}</div>
          <div class="setting-row__hint">{{ messages.settings.perDirTimeoutHint }}</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.per_dir_timeout_sec" class="input" type="number" min="5" max="300" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.totalBudget }}</div>
          <div class="setting-row__hint">{{ messages.settings.totalBudgetHint }}</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.scan_budget_sec" class="input" type="number" min="60" max="3600" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>{{ messages.settings.thresholdsTitle }}</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.activityWindow }}</div>
          <div class="setting-row__hint">{{ messages.settings.activityWindowHint }}</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.active_days" class="input" type="number" min="1" max="90" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.artifactThreshold }}</div>
          <div class="setting-row__hint">{{ messages.settings.artifactThresholdHint }}</div>
        </div>
        <div class="setting-row__control row-wrap">
          <input v-model.number="settings.min_artifact_mb" class="slider" type="range" min="0" max="500" step="10" style="max-width: 280px" />
          <span class="numeric tiny nowrap">{{ settings.min_artifact_mb }} {{ messages.units.megabytes }}</span>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.untrackedUnsafe }}</div>
          <div class="setting-row__hint">{{ messages.settings.untrackedUnsafeHint }}</div>
        </div>
        <div class="setting-row__control">
          <input v-model="settings.untracked_is_unsafe" class="switch" type="checkbox" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>{{ messages.settings.protectionTitle }}</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.whitelistPaths }}</div>
          <div class="setting-row__hint">{{ messages.settings.whitelistPathsHint }}</div>
        </div>
        <div class="setting-row__control">
          <textarea v-model="whitelistText" class="textarea" spellcheck="false" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.protectedBranches }}</div>
          <div class="setting-row__hint">{{ messages.settings.protectedBranchesHint }}</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model="protectedBranchesText" class="input input--mono" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>{{ messages.settings.deletionTitle }}</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.directoryDeletion }}</div>
          <div class="setting-row__hint">{{ messages.settings.directoryDeletionHint }}</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <select class="select" disabled>
            <option>{{ messages.settings.directoryDeletionOption }}</option>
          </select>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.worktreeDeletion }}</div>
          <div class="setting-row__hint">{{ messages.settings.worktreeDeletionHint }}</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <select class="select" disabled>
            <option>{{ messages.settings.worktreeDeletionOption }}</option>
          </select>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.strongProtection }}</div>
          <div class="setting-row__hint">{{ messages.settings.strongProtectionHint }}</div>
        </div>
        <div class="setting-row__control">
          <input v-model="settings.require_project_name" class="switch" type="checkbox" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>{{ messages.settings.logsTitle }}</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.operationLog }}</div>
          <div class="setting-row__hint">{{ messages.settings.operationLogHint(history.length) }}</div>
        </div>
        <div class="setting-row__control row-wrap">
          <input class="input input--mono input--sm" :value="logPath" readonly style="max-width: 360px" />
          <button class="btn" :title="messages.settings.prototypeTitle">{{ messages.settings.openDirectory }}</button>
          <button class="btn" disabled :title="messages.settings.clearLogTitle">{{ messages.settings.clearLog }}</button>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.appearance }}</div>
          <div class="setting-row__hint">{{ messages.settings.appearanceHint }}</div>
        </div>
        <div class="setting-row__control">
          <div class="segmented">
            <button :class="settings.theme === 'system' ? 'is-active' : ''" @click="settings.theme = 'system'">{{ messages.settings.system }}</button>
            <button :class="settings.theme === 'light' ? 'is-active' : ''" @click="settings.theme = 'light'">{{ messages.settings.light }}</button>
            <button :class="settings.theme === 'dark' ? 'is-active' : ''" @click="settings.theme = 'dark'">{{ messages.settings.dark }}</button>
          </div>
        </div>
      </div>
    </div>

    <p class="faint tiny">{{ " " + messages.settings.pathAbbrevBefore }} <span class="mono">{{ homePath(logPath) }}</span>{{ messages.settings.pathAbbrevAfter + " " }}</p>
  </div>
</template>

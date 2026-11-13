<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { history, projectPath, refreshHistory, settings } from "../state";
import { homePath } from "../format";
import { LOCALES, messages } from "../i18n";
import type { LocaleChoice } from "../i18n";
import { APP_VERSION } from "../version";
import {
  checkForUpdates,
  downloadPercent,
  installUpdate,
  installerRestartsApp,
  isBusy,
  restartApp,
  updateState
} from "../update";
import {
  clearLog as clearLogFile,
  isDesktop,
  logPath as resolveLogPath,
  revealInFileManager
} from "../platform";
import AppIcon from "../components/AppIcon.vue";

const desktop = isDesktop();

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

/** The language row: "follow system" plus every table the app ships. */
const localeChoices: LocaleChoice[] = ["system", ...LOCALES];

function localeLabel(choice: LocaleChoice): string {
  if (choice === "system") return messages.settings.locale.system;
  return choice === "en" ? messages.settings.locale.en : messages.settings.locale.zhCN;
}

/**
 * The appearance row.
 *
 * `data-theme-choice` and `data-locale` are stable hooks for the browser check:
 * it has to click these buttons whatever language the page happens to be in, and
 * a selector is the only thing that survives a translation.
 */
const themeChoices = ["system", "light", "dark"] as const;

function themeLabel(choice: (typeof themeChoices)[number]): string {
  return messages.settings[choice];
}

// The log path is asked for, not assumed: hardcoding one platform's layout is how
// the settings page ends up pointing at a file the app never writes.
const logFile = ref<string | null>(null);
const logNotice = ref<string | null>(null);

onMounted(async () => {
  logFile.value = await resolveLogPath();
  await refreshHistory();
});

const logDisplay = computed(() => {
  if (logFile.value) return logFile.value;
  return desktop ? messages.common.none : messages.app.desktopOnly;
});

async function openLogDirectory() {
  if (!logFile.value) return;
  try {
    await revealInFileManager(logFile.value);
  } catch (error) {
    logNotice.value = error instanceof Error ? error.message : String(error);
  }
}

async function onClearLog() {
  if (!logFile.value) return;
  // Emptying the audit trail is not undoable, so it asks first.
  const confirmed = window.confirm(messages.settings.clearLogTitle);
  if (!confirmed) return;
  try {
    await clearLogFile();
    await refreshHistory();
    logNotice.value = null;
  } catch (error) {
    logNotice.value = error instanceof Error ? error.message : String(error);
  }
}

// ---- updates ----------------------------------------------------------------

/**
 * What the update path is doing, in one sentence.
 *
 * Read at render time from the message table, like every other string: the
 * language can change while the app is running, and a copy taken at import time
 * would keep the old one.
 */
const updateStatus = computed(() => {
  const copy = messages.update;
  switch (updateState.phase) {
    case "unsupported":
      return copy.desktopOnly;
    case "checking":
      return copy.checking;
    case "available":
      return copy.available(updateState.version ?? "");
    case "downloading": {
      const percent = downloadPercent();
      return percent === null ? copy.downloadingUnknown : copy.downloading(percent);
    }
    case "ready":
      // On Windows the installer closes the app, so there is nothing left to
      // restart from here.
      return installerRestartsApp()
        ? copy.readyWindows
        : copy.ready(updateState.version ?? "");
    case "error":
      return copy.failed(updateState.error ?? "");
    default:
      return updateState.checkedAt ? copy.upToDate : copy.checking;
  }
});

/** The release notes that came with the update, if there were any. */
const updateNotes = computed(() => updateState.notes ?? "");
</script>

<template>
  <div class="stack page--wide">
    <div class="page-head">
      <div class="page-head__lead">
        <span class="page-head__icon"><AppIcon name="settings" /></span>
        <div class="page-head__text">
          <h1>{{ messages.settings.title }}</h1>
          <p class="page-head__sub">
            {{ messages.settings.currentProject }} <span class="mono">{{ projectPath || messages.common.none }}</span>
          </p>
        </div>
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
          <input class="input input--mono input--sm" :value="logDisplay" readonly style="max-width: 360px" />
          <button class="btn" :disabled="!logFile" @click="openLogDirectory">
            {{ messages.settings.openDirectory }}
          </button>
          <button class="btn" :disabled="!logFile" :title="messages.settings.clearLogTitle" @click="onClearLog">
            {{ messages.settings.clearLog }}
          </button>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.language }}</div>
          <div class="setting-row__hint">{{ messages.settings.languageHint }}</div>
        </div>
        <div class="setting-row__control">
          <div class="segmented">
            <button
              v-for="choice in localeChoices"
              :key="choice"
              :data-locale="choice"
              :class="settings.locale === choice ? 'is-active' : ''"
              @click="settings.locale = choice"
            >
              {{ localeLabel(choice) }}
            </button>
          </div>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.settings.appearance }}</div>
          <div class="setting-row__hint">{{ messages.settings.appearanceHint }}</div>
        </div>
        <div class="setting-row__control">
          <div class="segmented">
            <button
              v-for="choice in themeChoices"
              :key="choice"
              :data-theme-choice="choice"
              :class="settings.theme === choice ? 'is-active' : ''"
              @click="settings.theme = choice"
            >
              {{ themeLabel(choice) }}
            </button>
          </div>
        </div>
      </div>
      <p v-if="logNotice" class="notice notice--unsafe">{{ logNotice }}</p>
    </div>

    <div class="card">
      <div class="card__head"><h2>{{ messages.update.title }}</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.update.currentVersion }}</div>
          <div class="setting-row__hint">{{ messages.update.hint }}</div>
        </div>
        <div class="setting-row__control row-wrap">
          <span class="badge badge--neutral">v{{ APP_VERSION }}</span>
          <button class="btn" :disabled="isBusy()" @click="checkForUpdates()">
            {{ messages.update.check }}
          </button>
          <button
            v-if="updateState.phase === 'available'"
            class="btn btn--primary"
            @click="installUpdate()"
          >
            {{ messages.update.download }}
          </button>
          <button
            v-if="updateState.phase === 'ready' && !installerRestartsApp()"
            class="btn btn--primary"
            @click="restartApp()"
          >
            {{ messages.update.restart }}
          </button>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">{{ messages.update.status }}</div>
          <div class="setting-row__hint">{{ updateStatus }}</div>
        </div>
        <div class="setting-row__control">
          <p v-if="updateNotes" class="faint tiny">{{ updateNotes }}</p>
        </div>
      </div>
    </div>

    <p class="faint tiny">{{ " " + messages.settings.pathAbbrevBefore }} <span class="mono">{{ homePath(logDisplay) }}</span>{{ messages.settings.pathAbbrevAfter + " " }}</p>
  </div>
</template>

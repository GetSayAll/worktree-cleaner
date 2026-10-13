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
        <h1>Settings</h1>
        <p class="faint tiny">Current project: <span class="mono">{{ projectPath || "(none)" }}</span></p>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>Scan scope</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Extra scan paths</div>
          <div class="setting-row__hint">One per line; when empty, the defaults are used (~/MySrc, ~/dev, ~/Projects, ~/Code)</div>
        </div>
        <div class="setting-row__control">
          <textarea v-model="extraPathsText" class="textarea" spellcheck="false" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Maximum scan depth</div>
          <div class="setting-row__hint">Relative to the scan root; deeper is slower</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.max_depth" class="input" type="number" min="1" max="12" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Per-directory timeout</div>
          <div class="setting-row__hint">After a timeout the item is treated as Needs review and never as Safe to delete</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.per_dir_timeout_sec" class="input" type="number" min="5" max="300" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Total scan budget</div>
          <div class="setting-row__hint">Stops when the budget runs out and honestly reports the unscanned portion</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.scan_budget_sec" class="input" type="number" min="60" max="3600" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>Thresholds</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Activity window (days)</div>
          <div class="setting-row__hint">An item is only considered quiet if it has had no writes for longer than this; otherwise it is marked Needs review</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model.number="settings.active_days" class="input" type="number" min="1" max="90" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Artifact display size threshold</div>
          <div class="setting-row__hint">Artifacts smaller than this are hidden by default to reduce noise</div>
        </div>
        <div class="setting-row__control row-wrap">
          <input v-model.number="settings.min_artifact_mb" class="slider" type="range" min="0" max="500" step="10" style="max-width: 280px" />
          <span class="numeric tiny nowrap">{{ settings.min_artifact_mb }} MB</span>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Treat untracked files as unsafe</div>
          <div class="setting-row__hint">When enabled, a worktree containing untracked files is marked Must not delete</div>
        </div>
        <div class="setting-row__control">
          <input v-model="settings.untracked_is_unsafe" class="switch" type="checkbox" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>Protection rules</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Whitelist paths</div>
          <div class="setting-row__hint">One per line; matching directories never enter the cleanup plan</div>
        </div>
        <div class="setting-row__control">
          <textarea v-model="whitelistText" class="textarea" spellcheck="false" />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Protected branches</div>
          <div class="setting-row__hint">Comma-separated, wildcards supported; the default branch is detected and added automatically</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <input v-model="protectedBranchesText" class="input input--mono" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>Deletion method</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Directory deletion</div>
          <div class="setting-row__hint">Fixed to the system Trash; this tool provides no permanent deletion and no bypass</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <select class="select" disabled>
            <option>System Trash (macOS Trash / Windows Recycle Bin)</option>
          </select>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Worktree deletion</div>
          <div class="setting-row__hint">Always via git worktree remove; uses git worktree prune when the directory is gone</div>
        </div>
        <div class="setting-row__control setting-row__control--narrow">
          <select class="select" disabled>
            <option>git worktree remove (never rm -rf)</option>
          </select>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Strong protection</div>
          <div class="setting-row__hint">When enabled, the project directory name must be typed by hand before cleanup</div>
        </div>
        <div class="setting-row__control">
          <input v-model="settings.require_project_name" class="switch" type="checkbox" />
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head"><h2>Logs and appearance</h2></div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Operation log</div>
          <div class="setting-row__hint">Written locally and never online. {{ history.length }} records so far.</div>
        </div>
        <div class="setting-row__control row-wrap">
          <input class="input input--mono input--sm" :value="logPath" readonly style="max-width: 360px" />
          <button class="btn" title="Prototype does not call into the system">Open directory</button>
          <button class="btn" disabled title="Clearing is not available in the prototype">Clear log</button>
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row__label">
          <div class="setting-row__name">Appearance</div>
          <div class="setting-row__hint">System follows light/dark mode on macOS / Windows automatically</div>
        </div>
        <div class="setting-row__control">
          <div class="segmented">
            <button :class="settings.theme === 'system' ? 'is-active' : ''" @click="settings.theme = 'system'">System</button>
            <button :class="settings.theme === 'light' ? 'is-active' : ''" @click="settings.theme = 'light'">Light</button>
            <button :class="settings.theme === 'dark' ? 'is-active' : ''" @click="settings.theme = 'dark'">Dark</button>
          </div>
        </div>
      </div>
    </div>

    <p class="faint tiny">
      Paths are shown using abbreviations like <span class="mono">{{ homePath(logPath) }}</span>; the full absolute path is written out.
    </p>
  </div>
</template>

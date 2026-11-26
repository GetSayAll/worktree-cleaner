<script setup lang="ts">
import { computed } from "vue";
import { goTo, startScan, workspaceSnapshot } from "../state";
import { homePath } from "../format";
import { messages } from "../i18n";

const workspace = computed(() => workspaceSnapshot.value);
const repositories = computed(() =>
  [...(workspace.value?.repositories ?? [])].sort((a, b) => {
    if (a.worktree_count === null && b.worktree_count === null) return a.path.localeCompare(b.path);
    if (a.worktree_count === null) return 1;
    if (b.worktree_count === null) return -1;
    return b.worktree_count - a.worktree_count || a.path.localeCompare(b.path);
  })
);

function openRepository(path: string) {
  void startScan(path);
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
      <span>{{ warning }}</span>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.workspace.repositoriesTitle(repositories.length) }}</h2>
        <span class="faint tiny">{{ messages.workspace.oneAtATime }}</span>
      </div>
      <div class="card__body card__body--flush">
        <div v-for="repository in repositories" :key="repository.path" class="item-row item-row--plain">
          <div class="item-row__main">
            <div class="item-row__head">
              <span class="item-row__name">{{ repository.name }}</span>
              <span class="chip">{{ messages.workspace.worktrees(repository.worktree_count) }}</span>
            </div>
            <div class="item-row__meta">
              <span class="item-row__where mono" :title="repository.path">{{ homePath(repository.path) }}</span>
            </div>
            <div class="item-row__foot">
              <div class="item-row__reasons">
                <div class="reasons">
                  <div class="reason reason--neutral">
                    <span class="reason__mark">·</span>
                    <span class="reason__text">{{ messages.workspace.rowEvidence }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
          <div class="item-row__figures">
            <button class="btn btn--primary" @click="openRepository(repository.path)">
              {{ messages.workspace.scan }}
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

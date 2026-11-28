<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { executeErrorMessage, executeRetireClone, executing, goTo, snapshot } from "../state";
import ReasonList from "../components/ReasonList.vue";
import { displayPath, formatBytes, homePath } from "../format";
import { messages } from "../i18n";

const retirement = computed(() => snapshot.value?.clone_retirement ?? null);
const linked = computed(() =>
  (snapshot.value?.worktrees ?? []).filter((item) => !item.is_main)
);
const acknowledged = ref(false);
const root = computed(() => snapshot.value?.root ?? "");
const eligible = computed(() => retirement.value?.eligible === true);

// The confirmation is the start of a clone-level deletion, so open at its own
// top rather than inheriting the scroll position of the overview.
onMounted(() => {
  const content = document.querySelector(".content");
  if (content) content.scrollTop = 0;
});

function confirmRetire() {
  if (!eligible.value || !acknowledged.value || executing.value) return;
  void executeRetireClone();
}
</script>

<template>
  <div class="stack page--wide">
    <div class="spread">
      <div>
        <h1>{{ messages.retire.title }}</h1>
        <p class="faint tiny">{{ messages.retire.lede }}</p>
      </div>
      <button class="btn" :disabled="executing" @click="goTo('overview')">
        {{ messages.retire.back }}
      </button>
    </div>

    <p v-if="executeErrorMessage" class="notice notice--unsafe">{{ executeErrorMessage }}</p>

    <div v-if="retirement" class="card">
      <div class="card__head">
        <h2>{{ messages.retire.cloneTitle }}</h2>
        <span v-if="eligible" class="chip chip--protected">{{ messages.retire.eligible }}</span>
        <span v-else class="chip">{{ messages.retire.blockedBadge }}</span>
      </div>
      <div class="card__body">
        <div class="stack stack--tight">
          <div>
            <div class="faint tiny">{{ messages.retire.mainWorktree }}</div>
            <div class="mono">{{ homePath(retirement.main_path) }}</div>
          </div>
          <p v-if="!eligible" class="faint tiny">{{ messages.retire.requirements }}</p>
          <div>
            <div class="faint tiny">{{ messages.retire.linkedWorktrees(linked.length) }}</div>
            <div v-if="linked.length === 0" class="faint tiny">{{ messages.retire.noLinked }}</div>
            <div v-else class="stack stack--tight">
              <div v-for="item in linked" :key="item.path" class="row-wrap">
                <span class="mono">{{ displayPath(item.path, root) }}</span>
                <span class="faint tiny">{{ item.branch || messages.common.detached }}</span>
                <span class="faint tiny numeric">{{ formatBytes(item.size_bytes) }}</span>
              </div>
            </div>
          </div>
          <div v-if="!eligible" class="faint tiny">{{ messages.retire.blockers }}</div>
          <ReasonList :reasons="retirement.reasons" :limit="1" />
        </div>
      </div>
      <div class="card__foot">
        <p class="faint tiny">{{ messages.retire.trashNotice }}</p>
      </div>
    </div>

    <div v-if="eligible" class="actionbar">
      <label class="row-wrap tiny faint" style="gap: 6px">
        <input v-model="acknowledged" class="check" type="checkbox" :disabled="executing" />
        <span>{{ messages.retire.acknowledge }}</span>
      </label>
      <button
        class="btn btn--danger"
        :disabled="!acknowledged || executing"
        @click="confirmRetire"
      >
        {{ executing ? messages.retire.retiring : messages.retire.confirm }}
      </button>
    </div>
    <div v-else class="notice">
      {{ messages.retire.blocked }}
    </div>
  </div>
</template>

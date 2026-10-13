<script setup lang="ts">
import { computed, ref } from "vue";
import { startScan } from "../state";
import { mockRoot } from "../mock/snapshot";
import { formatBytes, homePath } from "../format";

const path = ref("");
const dragActive = ref(false);

const recents = [
  { path: mockRoot, scanned: "今天 09:12", safe: 8, review: 6, unsafe: 4, freed: 4.2 * 1024 ** 3 },
  {
    path: "/Users/andy/MySrc/private-marketing-toolkit",
    scanned: "3 天前",
    safe: 3,
    review: 1,
    unsafe: 2,
    freed: 1.1 * 1024 ** 3
  },
  { path: "/Users/andy/MySrc/worktree-cleaner", scanned: "11 天前", safe: 2, review: 0, unsafe: 1, freed: 612 * 1024 ** 2 }
];

const dropStyle = computed(() =>
  dragActive.value ? { borderColor: "var(--accent)", background: "var(--accent-weak)" } : undefined
);

function submit() {
  if (!path.value.trim()) return;
  startScan(path.value);
}

function fillDemoPath() {
  path.value = mockRoot;
}

function onDrop(event: DragEvent) {
  dragActive.value = false;
  const dropped = event.dataTransfer?.files?.[0] as unknown as { path?: string } | undefined;
  const text = dropped?.path ?? event.dataTransfer?.getData("text/plain") ?? "";
  if (text) path.value = text.trim();
}
</script>

<template>
  <div class="welcome">
    <div class="welcome__hero">
      <p class="faint tiny">Worktree Cleaner</p>
      <h1 class="welcome__title">项目清理审计</h1>
      <p class="welcome__lede">
        扫描一个仓库，列出它的全部 Git worktree 与构建产物，逐条说明为什么可以删、为什么不能删。默认只读扫描，删除一律进系统废纸篓。
      </p>
    </div>

    <div
      class="dropzone"
      :style="dropStyle"
      @dragover.prevent="dragActive = true"
      @dragleave="dragActive = false"
      @drop.prevent="onDrop"
    >
      <div class="pick-row">
        <input
          v-model="path"
          class="input input--mono"
          placeholder="/Users/andy/MySrc/your-repo"
          @keyup.enter="submit"
        />
        <button class="btn" title="原型中填入示例路径；正式版接系统目录选择器" @click="fillDemoPath">选择文件夹</button>
        <button class="btn btn--primary" :disabled="!path.trim()" @click="submit">开始扫描</button>
      </div>
      <p class="field__hint">也可以把项目文件夹拖进这个区域。扫描阶段不写任何文件。</p>
    </div>

    <div class="promise-grid">
      <div class="promise">
        <div class="promise__k">只读扫描</div>
        <div class="promise__v">检查阶段只跑 Git 只读命令，不改动仓库、不写入目标目录。</div>
      </div>
      <div class="promise">
        <div class="promise__k">可验证原因</div>
        <div class="promise__v">每条判断都附证据：具体命令、文件计数或路径，不接受「看起来像垃圾」。</div>
      </div>
      <div class="promise">
        <div class="promise__k">废纸篓删除</div>
        <div class="promise__v">目录删除统一走系统废纸篓，Worktree 走 git worktree remove，不永久删除。</div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>最近项目</h2>
        <span class="faint tiny">点击直接扫描</span>
      </div>
      <div class="card__body">
        <div class="recent-list">
          <button v-for="item in recents" :key="item.path" class="recent" @click="startScan(item.path)">
            <span class="recent__path">{{ homePath(item.path) }}</span>
            <span class="faint tiny nowrap">上次扫描 {{ item.scanned }}</span>
            <span class="row-wrap nowrap">
              <span class="tiny" style="color: var(--safe)">{{ item.safe }}</span>
              <span class="tiny" style="color: var(--review)">{{ item.review }}</span>
              <span class="tiny" style="color: var(--unsafe)">{{ item.unsafe }}</span>
            </span>
            <span class="faint tiny nowrap">可释放 {{ formatBytes(item.freed) }}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

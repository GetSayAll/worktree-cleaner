<script setup lang="ts">
import { ref } from "vue";

const projectPath = ref("");
const message = ref("初始项目骨架已创建。扫描器将在后续实现。");
</script>

<template>
  <main class="shell">
    <p class="eyebrow">GetSayAll · Worktree Cleaner</p>
    <h1>项目清理审计</h1>
    <p class="subtitle">跨 Agent 扫描 Git Worktree 和 artifacts，并说明是否可以安全删除。</p>
    <section class="toolbar">
      <input v-model="projectPath" placeholder="输入项目路径" />
      <button @click="message = projectPath ? '扫描器接口已预留。' : '请先输入项目路径。'">扫描项目</button>
    </section>
    <p class="message">{{ message }}</p>
    <div class="cards">
      <article><strong>Worktree</strong><span>分支、状态、最后更新时间和删除原因</span></article>
      <article><strong>Artifacts</strong><span>目录类型、大小、最后更新时间和删除原因</span></article>
      <article><strong>平台</strong><span>macOS / Windows · Vue 3 · Tauri 2 · Rust</span></article>
    </div>
  </main>
</template>

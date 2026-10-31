<script setup lang="ts">
import { computed, ref } from "vue";
import type { HygieneFinding, HygieneLevel, HygieneRule } from "../types";
import { snapshot } from "../state";
import { displayPath, hygieneRuleKey, hygieneText } from "../format";
import { messages } from "../i18n";
import StatCard from "../components/StatCard.vue";

// The policy page: what the audit found, and the wiring that keeps it from
// coming back. Every level here was decided in `crates/worktree-core/src/hygiene.rs`
// — this page orders and words the findings, it never re-derives a verdict (D9).

const report = computed(() => snapshot.value?.hygiene ?? null);
const root = computed(() => snapshot.value?.root ?? "");
const rules = computed(() => report.value?.rules ?? []);

/**
 * Findings that say something about this repository.
 *
 * The `not_checkable` ones are not about the repository at all — they are the
 * audit saying which rules it cannot see — so they belong in the scope block at
 * the bottom rather than mixed into the list of things to fix.
 */
const detail = computed(() =>
  (report.value?.findings ?? []).filter((finding) => finding.args.state !== "not_checkable")
);

/** Everything still open. Accepted exceptions are listed separately, with their date. */
const open = computed(() => detail.value.filter((finding) => finding.waived_until === undefined));
const accepted = computed(() => detail.value.filter((finding) => finding.waived_until !== undefined));

const counts = computed(() => {
  const tally = { violation: 0, advisory: 0, unknown: 0, ok: 0 };
  for (const status of rules.value) tally[status.level] += 1;
  return tally;
});

const groups = computed(() =>
  rules.value
    .map((status) => ({
      rule: status.rule,
      level: status.level,
      items: open.value.filter((finding) => finding.rule === status.rule)
    }))
    .filter((group) => group.items.length > 0)
);

/** The rules the scan cannot see, with the audit's own reason for each. */
const unchecked = computed(() =>
  rules.value
    .filter((status) => status.level === "unknown")
    .map((status) => ({
      rule: status.rule,
      why: (report.value?.findings ?? []).find(
        (finding) => finding.rule === status.rule && finding.args.state === "not_checkable"
      )
    }))
);

/**
 * The prevention half: four pieces of wiring, each tied to the rule it makes
 * true. Their state is the audit's, so a page that says "set this up" also says
 * whether it is already set up.
 */
const SETUP: Array<{ key: "ignore" | "cache" | "guard" | "close"; rule: HygieneRule; snippet: "gitignore" | "cache" | "guard" | "close" }> = [
  { key: "ignore", rule: "L5", snippet: "gitignore" },
  { key: "cache", rule: "B2", snippet: "cache" },
  { key: "guard", rule: "L6", snippet: "guard" },
  { key: "close", rule: "T2", snippet: "close" }
];

function levelOf(rule: HygieneRule): HygieneLevel {
  return rules.value.find((status) => status.rule === rule)?.level ?? "unknown";
}

const BADGE: Record<HygieneLevel, string> = {
  ok: "badge--safe",
  advisory: "badge--review",
  violation: "badge--unsafe",
  unknown: "badge--neutral"
};

const MARK: Record<HygieneLevel, string> = { ok: "✓", advisory: "!", violation: "✕", unknown: "–" };

// The `reason--*` tone for a level. A finding is drawn with the same anatomy as a
// reason on the worktree and artifact pages — mark, sentence, evidence — so the
// colour a reader already associates with "can go" / "look first" / "do not touch"
// means the same thing here. `unknown` gets a neutral tone of its own: it is not a
// shade of "fine".
const TONE: Record<HygieneLevel, string> = {
  ok: "safe",
  advisory: "review",
  violation: "unsafe",
  unknown: "neutral"
};

const copied = ref<string | null>(null);

async function copy(key: string, text: string) {
  try {
    await navigator.clipboard?.writeText(text);
    copied.value = key;
    window.setTimeout(() => {
      if (copied.value === key) copied.value = null;
    }, 1800);
  } catch {
    copied.value = null;
  }
}

function path(finding: HygieneFinding): string {
  const shown = displayPath(finding.subject, root.value);
  return shown === "." ? messages.common.mainWorktree(shown) : shown;
}

/**
 * The title of a rule, and the one-line fix that goes with it.
 *
 * Read at render time rather than captured in a module constant: a map built at
 * import keeps the language that was active then. Both look the rule up through
 * `hygieneRuleKey`, because the message table spells the one dotted id `B11`.
 */
function ruleTitle(rule: HygieneRule): string {
  const table: Record<string, string> = messages.hygiene.rules;
  return table[hygieneRuleKey(rule)] ?? rule;
}

function ruleFix(rule: HygieneRule): string {
  const table: Record<string, string> = messages.hygiene.fixes;
  return table[hygieneRuleKey(rule)] ?? "";
}
</script>

<template>
  <div class="stack page--wide">
    <div class="spread">
      <div>
        <h1>{{ messages.hygiene.title }}</h1>
        <p class="faint tiny">
          {{ messages.hygiene.metOf(counts.ok, rules.length) }}
          <span v-if="report"> · {{ messages.hygiene.policyRef }}</span>
        </p>
      </div>
    </div>

    <p class="faint tiny">{{ messages.hygiene.lede }}</p>

    <div v-if="snapshot" class="stat-grid stat-grid--4">
      <StatCard
        :label="messages.hygiene.stats.violation"
        tone="unsafe"
        value-tone
        :value="String(counts.violation)"
        :sub="messages.hygiene.level.violation"
      />
      <StatCard
        :label="messages.hygiene.stats.advisory"
        tone="review"
        value-tone
        :value="String(counts.advisory)"
        :sub="messages.hygiene.level.advisory"
      />
      <StatCard
        :label="messages.hygiene.stats.unchecked"
        :value="String(counts.unknown)"
        :sub="messages.hygiene.scope.title"
      />
      <StatCard
        :label="messages.hygiene.stats.met"
        tone="safe"
        value-tone
        :value="String(counts.ok)"
        :sub="messages.hygiene.level.ok"
      />
    </div>

    <div v-if="counts.violation > 0" class="banner banner--danger">
      <span>✕</span>
      <span>
        <span class="banner__title">{{ messages.hygiene.level.violation }}</span>
        {{ messages.hygiene.violationBanner(counts.violation) }}
      </span>
    </div>

    <!-- The findings, grouped by rule: the rule is the thing being fixed, and a
         list sorted by path would scatter one rule's instances through the page. -->
    <div v-for="group in groups" :key="group.rule" class="card">
      <div class="group-head">
        <span class="chip">{{ group.rule }}</span>
        <span class="group-head__path">{{ ruleTitle(group.rule) }}</span>
        <span style="flex: 1"></span>
        <span class="badge" :class="BADGE[group.level]">{{ messages.hygiene.level[group.level] }}</span>
      </div>
      <div class="card__body card__body--flush">
        <div v-for="finding in group.items" :key="finding.rule + finding.subject + finding.evidence" class="item-row item-row--plain">
          <div class="item-row__main">
            <div class="item-row__head">
              <span class="item-row__name" :title="finding.subject">{{ path(finding) }}</span>
            </div>
            <div class="reasons">
              <div class="reason" :class="`reason--${TONE[finding.level]}`">
                <span class="reason__mark">{{ MARK[finding.level] }}</span>
                <span class="reason__text">{{ hygieneText(finding) }}</span>
                <span class="reason__evidence">{{ finding.evidence }}</span>
              </div>
            </div>
            <div class="item-row__foot">
              <p class="tiny muted">{{ messages.hygiene.findings.fix }}: {{ ruleFix(finding.rule) }}</p>
            </div>
          </div>
          <div class="item-row__figures">
            <span class="badge" :class="BADGE[finding.level]">{{ messages.hygiene.level[finding.level] }}</span>
          </div>
        </div>
      </div>
    </div>

    <div v-if="groups.length === 0" class="card">
      <div class="empty">{{ messages.hygiene.findings.empty }}</div>
    </div>

    <!-- Accepted exceptions: the finding stays visible, with the date it stops
         being accepted. Hiding it would make a written waiver look like compliance. -->
    <div v-if="accepted.length > 0" class="card">
      <div class="card__head">
        <h2>{{ messages.hygiene.findings.acceptedTitle }}</h2>
      </div>
      <div class="card__body card__body--flush">
        <div v-for="finding in accepted" :key="finding.rule + finding.subject" class="item-row item-row--plain">
          <div class="item-row__main">
            <div class="item-row__head">
              <span class="chip">{{ finding.rule }}</span>
              <span class="item-row__name" :title="finding.subject">{{ path(finding) }}</span>
            </div>
            <div class="reasons">
              <div class="reason reason--safe">
                <span class="reason__mark">{{ MARK.ok }}</span>
                <span class="reason__text">{{ hygieneText(finding) }}</span>
                <span class="reason__evidence">{{ finding.evidence }}</span>
              </div>
            </div>
            <p class="faint tiny">{{ messages.hygiene.findings.acceptedUntil(finding.waived_until ?? "") }}</p>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.hygiene.setup.title }}</h2>
        <span class="faint tiny">{{ messages.hygiene.setup.note }}</span>
      </div>
      <div class="card__body stack">
        <div v-for="item in SETUP" :key="item.key" class="stack">
          <div class="gate">
            <div class="gate__item">
              <span
                class="gate__mark"
                :class="levelOf(item.rule) === 'ok' ? 'gate__mark--ok' : levelOf(item.rule) === 'unknown' ? '' : 'gate__mark--bad'"
              >{{ MARK[levelOf(item.rule)] }}</span>
              <span :class="levelOf(item.rule) === 'ok' ? 'muted' : ''">{{ messages.hygiene.setup.items[item.key] }}</span>
              <span style="flex: 1"></span>
              <button class="btn btn--ghost" @click="copy(item.snippet, messages.hygiene.snippets[item.snippet])">
                {{ copied === item.snippet ? messages.common.copied : messages.hygiene.copy }}
              </button>
            </div>
          </div>
          <pre class="code">{{ messages.hygiene.snippets[item.snippet] }}</pre>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.hygiene.scope.title }}</h2>
      </div>
      <div class="card__body stack">
        <p class="faint tiny">{{ messages.hygiene.scope.note }}</p>
        <div class="gate">
          <div v-for="entry in unchecked" :key="entry.rule" class="gate__item">
            <span class="gate__mark">–</span>
            <span>
              <span class="chip">{{ entry.rule }}</span>
              {{ ruleTitle(entry.rule) }}
              <span v-if="entry.why" class="faint"> — {{ hygieneText(entry.why) }}</span>
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

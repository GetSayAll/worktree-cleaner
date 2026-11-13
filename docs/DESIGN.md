# Visual design

Status: frozen for v0.1. The welcome screen is the reference implementation; every
other page is a variation of it.

This file is the visual contract for the interface. It covers the shell, the type
and spacing scales, the component anatomy, and how each page is composed. The code
that implements these rules lives in `src/styles.css`, `src/App.vue`, and
`src/views/`. When a rule here and a rule in the stylesheet disagree, the stylesheet
is wrong unless this file is updated in the same change.

## 1. What the interface is

Worktree Cleaner is a local audit tool. The reader is looking at paths, branch
names, byte counts, and verdicts. The interface has to make those facts easy to
scan and has to stay quiet around them. It is not a marketing page and not a
dashboard of decorative charts.

The visual language follows four reference points:

- macOS native windows: one material for chrome, one white content sheet, a
  hairline between regions, and one accent that follows the system colour.
- Apple HIG typography and materials: hierarchy comes from size, weight, and one
  surface step, not from many colours.
- Premium editorial layout: generous margins, short copy, one focal action per
  screen, and groups separated by whitespace before borders are added.
- Evidence-first tooling: the verdict is the only thing allowed to spend colour.

## 2. Principles

### P1. The content pane is white, never grey

In the light theme the main pane, the sidebar, and a card are all `--surface`
white. A card is separated by a `1px` hairline and one shadow step, not by putting
it on a grey canvas. The dark theme is the exception: there a sheet sits a few
points above the canvas so it still reads as raised.

No page may reintroduce a large grey background, a tinted work surface, or a
decorative gradient behind the content.

### P2. One accent per screen

The accent marks the brand, the active navigation row, the single primary action,
and the keyboard focus ring. Everything else is ink, hairline, or white.

`--safe`, `--review`, and `--unsafe` are spent only on verdicts, their dots, and
their words. A row never tints its own background to repeat a verdict that is
already sitting at the end of that row.

### P3. Type carries the hierarchy

There are eight type steps and no ninth. The page title is the largest text in the
header; the only larger figure is a stat card's metric. A card title is one step
above its body; captions are metadata, not decoration. Weight and tracking
separate levels before size is increased.

Latin display text is tightened under `:lang(en)`. Chinese keeps its designed
tracking and gets a slightly looser line height.

### P4. Group with space, then with a hairline

Whitespace is the first grouping tool. A hairline is the second. A box inside a
box is the last resort, and a card inside a card is not allowed.

Lists that belong together share one surface and are divided by inset hairlines.
Recessed areas are only used where the reader must understand that something is
secondary, such as a code block or a detail flyout.

### P5. Native density, native quiet

The app is read at a desk, not on a phone. Rows are about 44–56 px tall, controls
are 30–40 px, and the content pane uses 32 px horizontal margins. Hover and focus
are visible, but motion is limited to a colour change and a 1–2 px shift.

### P6. The evidence is the product

A page may say "safe" only with a reason beside it. A banner may warn only when
the core returned a violation, a blocker, or an execution failure. The visual
system never adds a claim of its own.

## 3. Tokens

The source of truth is the light `:root` block in `src/styles.css` and the two dark
copies below it. `src/theme.test.ts` fails when the two dark copies drift or when a
layout token leaks into a theme override.

### Colour

| Token | Job |
|---|---|
| `--bg` | The application canvas. White in the light theme. |
| `--surface` | A sheet: sidebar, card, composer, page column. |
| `--surface-2` | A recessed strip: code block, trace, table header hover, control track. |
| `--surface-3` | A control track or hover fill. |
| `--border` | The hairline between regions. |
| `--border-strong` | A control edge that must be found without hover. |
| `--text` | Primary ink. |
| `--text-2` | Supporting prose and metadata. |
| `--text-3` | Captions, fine print, placeholders. |
| `--accent` | The system accent. Everything else is mixed from it. |
| `--safe` / `--review` / `--unsafe` | Verdicts only. |

Secondary ink must clear 4.5:1 on every surface. The theme test enforces this.

### Type

| Token | Size | Job |
|---|---|---|
| `--fs-caption` | 12 px | metadata, chips, evidence, table headers, fine print |
| `--fs-ui` | 13 px | navigation, controls, table cells, row names |
| `--fs-body` | 14 px | prose, banners, key/value lists |
| `--fs-title` | 16 px | card headings |
| `--fs-lead` | 18 px | the welcome screen's one paragraph |
| `--fs-page` | 22 px | page headings |
| `--fs-metric` | 26 px | the figure on a stat card |
| `--fs-hero` | 32 px | the welcome screen's title |

Never write a numeric `font-size`. Add a step only by editing this table, the
stylesheet, and `src/typography.test.ts` in the same commit.

### Spacing

`4 / 8 / 12 / 16 / 20 / 24 / 32 px` (`--sp-1` … `--sp-7`). A card body uses 16/20;
a page uses 32 horizontal; a stack uses 16 or 24. Do not introduce a sixth value
beside the scale.

### Shape and depth

- radii: `8` controls, `10` toolbar/segmented, `14` cards and the composer.
- `--shadow`: a card at rest.
- `--shadow-lift`: a floating card, the composer, and the action bar.
- `--inset`: the top highlight that keeps a dark sheet from looking like a hole.

## 4. Shell

### Sidebar

- Width 220 px. `background: var(--surface)`, one right hairline.
- Top: a 30 px app mark, the product name at 14/600, and the version at 12.
- Navigation is read in three passes: the way in (`Project`), `Audit`, and
  `Cleanup`. A small caption names each group; there is no divider ladder.
- A row is 32 px, with a 16 px line icon, the label, and an optional count
  capsule. Counts appear only when they are non-zero.
- Active: `--accent-weak` field, `--accent-text` label and icon. No coloured edge
  bar.
- `Settings` sits at the foot above the fine print. The fine print is separated by
  a hairline and is written in one or two short lines.

### Topbar

- 56 px tall, transparent over the canvas, one bottom hairline.
- Left: a folder mark and the project path in the mono face. When no project is
  open, the same slot says "No project selected".
- Right: optional state badges, then the project actions grouped together. The
  update banner is the only button allowed to interrupt from this bar.
- The bar is a drag region on macOS; interactive children keep their clicks.

### Content

- Padding `24px 32px 32px`.
- One page column per page; no page is narrower than the pane at the default
  window. The welcome page may cap its inner column at 1040 px on a wide monitor.
- Only `.content` scrolls.

## 5. Page anatomy

Every page except the welcome screen starts with the same header:

```
.page-head
  .page-head__lead
    .page-head__icon      36 px neutral tile with a 19 px line icon
    .page-head__text
      h1                  22/600
      .page-head__sub     13, --text-3; mono when it is a path
  .page-head__actions     right-aligned buttons, wrap before overflow
```

The icon names the page and repeats the section's navigation mark, so the reader
always knows which section they are in. The page-head tile is neutral, not
accent-filled; the sidebar and welcome marks are the app icon — the same blue
rounded-square branch mark as the Dock icon — and are the only brand-colored tiles.

Below the header a page composes, in order:

1. an optional toolbar for filters, sort, search, or a summary;
2. an optional page note of one or two lines;
3. stat cards, banners, cards, lists, or tables;
4. a sticky action bar when there is a next or destructive step.

### Toolbar

A white card at `--radius`, one hairline, 10 px vertical and 12 px horizontal
padding. It holds the segmented control, a search field, a slider group, and a
right-aligned count or summary. It is a control surface, not a second page card.

### Card

- White sheet, `1px` hairline, `--radius-lg`, `--shadow`.
- Head: `14px 20px`, minimum 48 px, title at 16/600 and an optional 12 px meta.
- Body: `16px 20px`; a flush body is used when the content is a list or a table.
- Foot: `12px 20px`, top hairline, no filled background.
- A card never sits inside another card. A nested panel uses a row, a detail
  flyout, or a code block instead.

### Inset list row

Used by recent projects, worktrees, artifacts, hygiene findings, and the
timeline.

- `14px 20px`, one top hairline between rows, hover `--surface-2`.
- Identity line: 13 px mono name, optional chip, optional badge.
- Meta line: 12 px supporting facts, separated by middle dots.
- Foot: evidence and the control that acts on the row.
- Figures sit on the right edge, never over the text.
- The name is one line. A long path ellipsizes; it never wraps into a paragraph.

### Table

Only the cleanup plan and the execution result use a real table.

- Header: 12 px caption, `--text-3`, transparent background, bottom hairline.
- Cells: 11 px vertical and 16 px horizontal, one bottom hairline per row.
- Hover: `--surface-2`. No zebra striping, no vertical rules.
- Numeric columns are right-aligned and tabular.

### Settings row

- `16px 20px`, one bottom hairline between rows.
- Label column 216 px; name 13/500, hint 12/`--text-3`.
- Control column fills the rest. A narrow control stops at 320 px.
- A switch is a 32 × 19 track with a 15 px knob; a segmented control is a
  recessed track with one raised active tab.

### Action bar

Sticky at the bottom of the content pane, opaque white, `--radius-lg`,
`--shadow-lift`. Left is back, middle is a one-line summary, right is forward or
the destructive action. Buttons on one row share a height.

### Empty state

A centered 36 px neutral icon tile above one line of 13 px `--text-3`. Inside a
table cell or a compact row, the icon shrinks to 24 px and sits beside the text.

## 6. Components

### Buttons

- 30 px high, 8 px radius, 13 px label, 500 weight.
- Secondary: white sheet, `--border-strong` edge, hover `--surface-3`.
- Primary: system accent gradient, white label, one inner highlight and one soft
  accent shadow. Exactly one primary button per visible action group.
- Danger: red gradient, used only for execution.
- Ghost: no edge, used inside card heads and rows.
- Disabled: a primary returns to a neutral `--surface-3` field with `--text-3`
  letters. It must not look like a faded active control.

### Banners and notices

- A 1 px border, 10 px radius, and a tinted semantic background for success,
  warning, and danger; a neutral notice stays on the white sheet.
- The mark is a 16 px icon, not a text glyph.
- The title is 600 and sits before the sentence on the same line.

### Badges and chips

- Badge: 12 px label, 5 px radius, a 5 px dot when it carries a verdict.
- Chip: mono 12, `--surface-3` fill, one hairline, ellipsizes.
- The topbar turns verdict badges into neutral counts; the tint belongs to the
  page where the decision is made.

### Form controls

- 32 px high, 8 px radius, one hairline, inset 1 px shadow.
- Focus: accent edge plus a 3 px `--ring`.
- Textareas are mono, 6 px radius, and resize vertically.
- Search fields carry a 14 px leading icon and 30 px left padding.

### Code and trace

- Mono 12, `--surface-2`, one hairline, 10 px radius.
- Code wraps only at intentional break points; trace lines ellipsize.
- A plan command keeps its surrounding card white so the command is the darkest
  block on the row.

### Gates

A gate is one line: a 18 px circular mark, the label, and an optional action.
Met is `--safe` on `--safe-bg`; failed is `--unsafe` on `--unsafe-bg`; pending is
`--text-3` on `--surface-2`.

## 7. Page recipes

| Page | Page icon | Composition |
|---|---|---|
| Welcome | large app mark | hero, command composer, three-part assurance sheet, recent projects inset list |
| Overview | grid | page head, four stat cards, banners, largest worktrees and artifacts cards, action bar |
| Worktrees | branch | page head, hygiene banner, toolbar (status segments + search), grouped list with detail flyout, action bar |
| Build artifacts | cube | page head, toolbar (sort + size slider + summary), collapsible group cards, action bar |
| Disk hygiene | shield | page head, page note, four stat cards, violation banner, rule group cards, setup card, scope card |
| Cleanup plan | checklist | page head, four stat cards, gates, two channel cards, confirmation, action bar |
| Result | check circle | page head with actions, four stat cards, result table, action bar |
| History | clock | page head, toolbar (type segments), timeline with dots, empty state |
| Settings | sliders | page head, one card per settings group, rows in the anatomy above |
| Scanning | search | page head, stage card with circular stage marks, trace, budget note |

## 8. Motion and accessibility

- Motion is a colour transition of 120–160 ms. The only continuous animation is
  the busy indicator.
- `prefers-reduced-motion` disables the spinner and the sweep and leaves a static
  busy state.
- Every control is reachable by keyboard and has a visible focus ring.
- Drag-and-drop is a shortcut, never the only way in. The composer has a typed
  path and a folder picker.
- A status is never carried by colour alone: it always has a word or an icon.

## 9. Enforcement

- `src/typography.test.ts`: the eight-step scale and no numeric font sizes.
- `src/theme.test.ts`: dark copies match, layout tokens stay out of themes, and
  secondary ink clears AA on every surface.
- `src/i18n/locale.test.ts`: every locale table has exactly the English key set.
- `pnpm test:layout`: every page has the same margins, rows do not overflow,
  names stay on one line, and every row carries its evidence.
- `pnpm test:theme`: all six OS/theme combinations resolve to the palette the
  stylesheet declares.

When a page needs a new pattern, add it to this file and to the shared stylesheet
before adding it to the page. A page-local style is a design decision that has not
been made yet.

## 10. Do not

- put the right pane on a grey or tinted canvas;
- use a card inside a card, or a bordered row inside a bordered row;
- tint a list row to repeat its verdict;
- introduce a second accent or a decorative gradient;
- add a font size, spacing value, radius, or colour outside the tokens;
- use a text glyph where the shared icon set has a mark for banners and gates;
- hide a deletion behind a hover-only control;
- let the interface make a safety claim the core did not make.

---
name: GitBound
description: A quiet, system-themed operational console for safe repository identity management.
colors:
  bg: "#f3f4f0"
  surface: "#fafbf8"
  surface-2: "#eef0eb"
  surface-3: "#e5e8e1"
  text: "#20241f"
  muted: "#687067"
  faint: "#626a62"
  line: "#d4d8d0"
  accent: "#3f6a50"
  accent-strong: "#31563f"
  accent-soft: "#dce9df"
  accent-foreground: "#f8fbf8"
  danger: "#a33e38"
  danger-soft: "#f5dfdd"
  warning: "#765317"
  warning-soft: "#f5ead2"
  dark-bg: "#111411"
  dark-surface: "#181c18"
  dark-surface-2: "#202520"
  dark-surface-3: "#2a302a"
  dark-text: "#edf0ea"
  dark-muted: "#aeb6ad"
  dark-faint: "#7f887f"
  dark-line: "#343b34"
  dark-accent: "#86b896"
  dark-accent-strong: "#a5c9b0"
  dark-accent-soft: "#263d2e"
  dark-danger: "#f0958e"
  dark-danger-soft: "#422523"
  dark-warning: "#e3bc73"
  dark-warning-soft: "#3d3321"
typography:
  display:
    fontFamily: 'ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
    fontSize: "28px"
    fontWeight: 700
    lineHeight: 1.15
    letterSpacing: "-0.025em"
  headline:
    fontFamily: 'ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
    fontSize: "20px"
    fontWeight: 700
    lineHeight: 1.5
    letterSpacing: "normal"
  title:
    fontFamily: 'ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
    fontSize: "18px"
    fontWeight: 700
    lineHeight: 1.5
    letterSpacing: "-0.015em"
  body:
    fontFamily: 'ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
  label:
    fontFamily: 'ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
    fontSize: "11px"
    fontWeight: 650
    lineHeight: 1.5
    letterSpacing: "normal"
  mono:
    fontFamily: '"Cascadia Mono", "SFMono-Regular", Consolas, monospace'
    fontSize: "0.92em"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "normal"
rounded:
  chip: "5px"
  control: "7px"
  row: "8px"
  data: "9px"
  panel: "12px"
  setup: "14px"
  pill: "999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  field: "14px"
  lg: "18px"
  xl: "24px"
  workspace: "32px"
components:
  button-primary:
    backgroundColor: "{colors.accent-strong}"
    textColor: "{colors.accent-foreground}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "6px 12px"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "6px 12px"
  status-badge:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.muted}"
    typography: "{typography.label}"
    rounded: "{rounded.pill}"
    padding: "3px 8px"
  field:
    backgroundColor: "{colors.bg}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 10px"
---

# Design System: GitBound Desktop

## Overview

**Creative North Star: "The Quiet Operational Console"**

GitBound is a dense, system-themed desktop utility, not a marketing surface. Its visual hierarchy keeps repository context, expected-versus-actual identity, and safe next actions visible at the same time. Neutral layered surfaces and crisp Lucide icons carry most of the structure; muted moss is reserved for identity, protection, selection, and success. Elevation is restrained, decoration is absent, and copy is calm and explicit.

The information architecture is a persistent title bar, primary sidebar, and scrollable workspace. The six peer views are Dashboard, Identities, Repositories, SSH Keys, Diagnostics, and Settings. Dashboard is the default: it answers "am I about to commit as the wrong person" without any clicking, and is read-mostly, so glancing at it can change nothing. Identities is a card grid, because the question it answers — which of these am I using, and where — is a comparison. Repositories is a full-width table, and every write moved out of it into the Repository details page it links to. SSH Keys, Diagnostics, and Settings use task-specific panels. Status remains as the deep per-check page the Dashboard and details pages link into, reachable but not a sidebar peer.

Quick Switch is a `Ctrl`/`Cmd`+`K` palette layered over any view. It is the only global shortcut, and it arms rather than applies: choosing an identity shows exactly what will change and a second, deliberate action commits. Fast to reach, never fast to fire.

Onboarding and load failure replace the workspace until a safe operating state exists. Toasts report transient outcomes without becoming the sole record of repository state.

**Key Characteristics:**

- Dense but legible source-control list/detail composition.
- Honest local state and expected-versus-actual comparisons.
- System light/dark themes with no decorative imagery.
- Explicit previews, confirmations, cancellation, and recovery copy.
- Status communicated through icon, label, and color together.

## Colors

The palette is a warm neutral system with a low-saturation moss accent. CSS custom properties in `desktop/ui/css/app.css` are the runtime source of truth. They swap under `prefers-color-scheme: dark` and can be overridden by an explicit `data-theme` attribute on the root element, which the Settings view sets; the frontmatter above records both confirmed schemes.

- **Moss:** `accent`, `accent-2`, and `accent-soft` mark the active route, primary safe action, selection edge, and protected state. A violet accent is offered as an alternative in Settings.
- **Status green:** `ok` and `ok-soft` mark a passing check. These are deliberately _not_ the accent: status colour has to keep meaning "this passed" when the accent changes, and an accent that is not green would otherwise silently repaint every passing check.
- **Warm neutrals:** `bg`, three surface levels, `text`, `muted`, `faint`, and `line` create hierarchy primarily through tone and one-pixel separators.
- **Brick danger:** `danger` and `danger-soft` identify failed checks, destructive actions, and blocking errors.
- **Ochre warning:** `warning` and `warning-soft` identify drift, unbound repositories, attention states, and rebind previews.

**The Semantic Color Rule.** Accent means protected, selected, or the primary safe action; `ok` means a check passed; warning and danger are not decorative alternatives. Status colour never depends on which accent is chosen.

**The Redundant Status Rule.** Every state retains a readable word and a status icon; color never carries meaning alone.

## Tokens

Every value in the frontmatter above exists as a CSS custom property in `desktop/ui/css/app.css`: `--space-1` through `--space-8` on a 4px scale, `--radius-chip` through `--radius-pill`, `--font-display` through `--font-micro` with their tracking, and `--elev-1` through `--elev-3`. Before v3 only colour was tokenised and every gap, pad, and corner was a bare pixel value, which is why there were sixteen distinct radii. New work uses the scale; a one-off value needs a reason.

## Typography

The interface uses the operating system sans-serif stack for native familiarity and a Cascadia/SFMono/Consolas stack for paths, configuration values, and remediation text. Type is compact and matter-of-fact; hierarchy comes from weight, size, and muted color rather than multiple families.

- **Display:** 28px page and onboarding titles with a tight line height and slight negative tracking.
- **Headline:** 20px setup-panel titles.
- **Title:** 18px repository, settings, and diagnostic headings.
- **Body:** 14px default copy at 1.5 line height; page descriptions stop at 68 characters.
- **Label:** 10–12px operational metadata, table headings, field labels, status words, and badges; uppercase is limited to pane and table labels.
- **Mono:** repository paths, approved roots, commands, and remediation content.

**The Operational Hierarchy Rule.** Reserve the 28px display role for the current task; repository facts and controls remain compact enough to scan together.

## Layout

The desktop shell is a 48px title bar over a 220px sidebar and flexible workspace. Workspace content is centered up to 1200px with 32px horizontal padding. The principal master/detail container uses a `minmax(260px, 35%)` list column and flexible inspector, with a 510px minimum height. Identity facts use a two-column definition grid; settings and diagnostics use stacked bordered sections. Actions sit at the top-right of page headers or directly beside the state they change.

At 860px and below, the sidebar becomes a 58px icon rail, labels and sidebar footer are removed, the list/detail inspector stacks, and its list is capped at 240px. Onboarding becomes one column, the title-bar locality message is hidden, and wide status tables scroll horizontally. At 620px and below, page headers, section headers, status controls, forms, and identity grids become one column; actions wrap; workspace padding contracts to 12px and inspector padding to 18px. The narrow layout must preserve repository status, recovery information, and action context rather than hiding them.

**The Context-Before-Action Rule.** Keep the selected repository/profile, its actual state, and the proposed target visible in the same scroll context as the mutation control.

## Elevation & Depth

Depth is mostly structural: background tone changes, one-pixel borders, divided rows, and an inset moss selection edge. Elevation is expressed as three tokens and each has one job: `--elev-1` lifts a card that can be acted on, `--elev-2` a popover or toast, `--elev-3` a modal that owns the screen. Nothing else casts a shadow.

Page changes enter with a 380ms low-amplitude fade/translate/blur, while working states use a simple spinner. `prefers-reduced-motion: reduce` collapses animation duration and disables smooth scrolling.

**The Flat-by-Default Rule.** Do not add shadows to routine rows, fields, badges, alerts, or navigation; use tonal layering and borders first. Cards and modals are the exception, and only because elevation there is functional — it says "this is a distinct object you can act on" and "this owns the screen until you dismiss it" — never decorative.

## Shapes

The form language is gently rounded and compact: 7px controls, 8–10px rows and data groups, 12–14px major panels, and fully rounded status badges. Borders are one pixel and low contrast. Circular geometry is reserved for status dots and numbered onboarding steps; the branch mark and avatars use small rounded squares. Dashed borders are reserved for the repository folder chooser.

## Components

### Shell and navigation

The title bar carries the GitBound mark, version, the Quick Switch trigger with its shortcut, and the local-configuration assurance. The sidebar keeps all six destinations stable, and its footer shows the active identity — collapsing to the avatar alone on the narrow icon rail, because that is the one thing the footer is for. Active navigation uses a soft moss fill and strong moss text; drift adds a warning dot. At narrow widths, icon-only navigation retains accessible names.

### Page headers and actions

Each view begins with one task title, one explanatory sentence, and optional right-aligned actions. Primary buttons are moss-filled; secondary buttons are neutral bordered; destructive text actions use brick without a filled danger block. All controls use a visible 2px moss `:focus-visible` outline with 2px offset, disabled styling, and text labels where space permits.

### Master/detail lists

Rows combine a 15–18px icon, strong name, truncated secondary path/account, and textual status. Hover uses the secondary surface; selection adds the same surface plus a 2px inset moss edge. The inspector begins with entity name, path, and badge, then shows alerts, facts, and controls in that order. Empty inspectors explain what to select and why.

### Status and feedback

Badges pair a Lucide status icon with a word in a semantic soft background. Inline alerts, health banners, test results, and rebind previews use the same icon/text/color grammar at larger scale. Error toasts use `role="alert"`; success toasts use `role="status"`. Loading, empty, no-result, and read-failure states have explicit copy. Data comparisons use a real table with row headers; narrow layouts scroll it rather than removing columns.

### Fields, panels, and code

Labels own their inputs and selects; forms use a two-column grid that collapses to one. Fields are 35px high with a one-pixel border and 7px corners. Search integrates its icon inside the field shell. Definition grids, settings sections, diagnostics, approved-root chips, and remediation code use consistent borders, surface levels, and mono treatment for machine-readable values.

### Safety and confirmation

Repository binding is a two-step preview/confirm flow that states which local values change, calls out forced rebind of drifted values, and preserves the unbind recovery promise. Unbind explicitly asks whether to restore the exact pre-bind settings. Profile removal warns that bound repositories will become missing-profile states. Approved-root removal requires a second activation. GitHub CLI switching previews current and target accounts and remains separate from binding. Every confirmation offers cancellation.

Network and external checks are initiated explicitly: SSH testing states that it made one requested connection, and Status distinguishes local-only results from a user-triggered network refresh. Initial read failure blocks write actions and routes users to retry or diagnostics. Onboarding inspects only the chosen folder and repeats that no broader disk scan occurs.

## Do's and Don'ts

### Do:

- **Do** preserve the five-view architecture and reuse the master/detail pattern for identity collections.
- **Do** source runtime visual values from the CSS custom properties; update this document when those shared tokens or breakpoints change.
- **Do** keep light and dark semantic roles paired and verify both schemes at desktop and narrow widths.
- **Do** give every icon-only action an accessible name, every field a label, every result persistent visible text, and every keyboard target a visible focus state.
- **Do** show expected, actual, proposed, and recoverable state before consequential actions, with explicit success or error feedback afterward.
- **Do** preserve the exact responsive contracts at 860px and 620px unless a tested replacement is documented.

### Don't:

- **Don't** add decorative illustration, gradients, glass effects, saturated brand color, or elevation to routine surfaces.
- **Don't** use color, a toast, or an unlabeled icon as the only carrier of status or action meaning.
- **Don't** silently bind, force-rebind, unbind, remove, switch GitHub CLI accounts, scan unapproved roots, or perform network checks.
- **Don't** combine repository binding with GitHub CLI switching or imply that GitBound stores credentials.
- **Don't** hide status, recovery copy, or confirmation context to make narrow layouts simpler; stack or scroll the content instead.
- **Don't** introduce a new spacing, radius, icon, or component variant for a one-off screen when an existing pattern serves the same role.

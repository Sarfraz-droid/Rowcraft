---
name: Rowcraft
description: Compact desktop database workbench
colors:
  dark-bg: "oklch(0.2195 0.0077 285.7448)"
  dark-panel: "oklch(0.2603 0 0)"
  dark-surface: "oklch(0.3703 0.0119 285.8054)"
  dark-text: "oklch(0.9702 0 0)"
  dark-muted: "oklch(0.7058 0 0)"
  dark-border: "oklch(0.4748 0 0)"
  dark-hover: "oklch(0.3703 0.0119 285.8054)"
  dark-selection: "#1e3a5f"
  dark-accent: "#2563eb"
  dark-error: "oklch(0.6122 0.2082 22.2410)"
  dark-success: "#7cc8a8"
  dark-syn-keyword: "#b6a1e0"
  dark-syn-string: "#a8c6a5"
  dark-syn-number: "#dbb487"
  dark-syn-type: "#90c4d1"
  light-bg: "oklch(1.0000 0 0)"
  light-panel: "oklch(0.9940 0 0)"
  light-surface: "oklch(0.9702 0 0)"
  light-text: "oklch(0 0 0)"
  light-muted: "oklch(0.4386 0 0)"
  light-border: "oklch(0 0 0)"
  light-hover: "oklch(0.9702 0 0)"
  light-selection: "#dbeafe"
  light-accent: "#2563eb"
  light-error: "oklch(0.6549 0.1689 31.5247)"
  light-success: "#28745b"
  light-syn-keyword: "#7855a8"
  light-syn-string: "#397544"
  light-syn-number: "#98521f"
  light-syn-type: "#28728b"
typography:
  body:
    fontFamily: '"Space Grotesk", ui-sans-serif, sans-serif, system-ui'
    fontSize: "12px"
    fontWeight: 400
  title:
    fontFamily: '"Space Grotesk", ui-sans-serif, sans-serif, system-ui'
    fontSize: "17px"
    fontWeight: 600
    letterSpacing: "-.035em"
  label:
    fontFamily: '"Space Grotesk", ui-sans-serif, sans-serif, system-ui'
    fontSize: "11px"
  data:
    fontFamily: '"Space Mono", ui-monospace, monospace'
    fontSize: "11px"
rounded:
  compact: "0px"
  control: "0px"
  small: "0px"
  map-table: "0px"
  dialog: "0px"
spacing:
  compact: "0px"
  control-gap: "12px"
  pane: "16px"
  section: "24px"
components:
  button-primary:
    backgroundColor: "{colors.dark-accent}"
    textColor: "#fff"
    rounded: "{rounded.control}"
    padding: "7px 11px"
  button-secondary:
    backgroundColor: "{colors.dark-bg}"
    textColor: "{colors.dark-text}"
    rounded: "{rounded.control}"
    padding: "7px 11px"
  navigation-selected:
    backgroundColor: "{colors.dark-selection}"
    textColor: "#dbeafe"
    rounded: "{rounded.small}"
    padding: "5px 8px"
---

# Design System: Rowcraft

## Overview

**Creative North Star: "Developer workbench"**

Rowcraft uses a compact developer workbench: a persistent source navigator, closable documents across sources, and dedicated views for SQL, data, and database structure. The Retro theme uses Space Grotesk for controls and Space Mono for SQL and data. Monochrome surfaces, square corners, and a blue primary color keep the workbench compact and readable.

This document records the implemented UI in the desktop frontend. Its descriptive language is inferred from source, rather than a separately approved brand concept. No image comp defines this system. Browser visual verification is pending; these values were extracted from code.

**Key Characteristics:**

- Compact navigation and toolbars
- Self-hosted monospace typography
- Theme-aware surfaces and restrained blue actions
- Charts and relationships derived from returned data

## Colors

The frontmatter records both explicit themes. The application sets both `data-theme` and shadcn’s `.dark` class on the root, using a saved preference or the system preference initially. Components consume Tailwind semantic tokens; the dark component examples in the frontmatter describe one theme, not fixed colors for both.

### Primary

Workbench blue identifies primary actions, active view underlines, selected source navigation, schema relationships, and positive result bars. Primary actions use blue with white text in both modes. Selection uses a pale blue fill with dark blue text in light mode and a deep blue fill with pale text in dark mode.

### Neutral

Graphite canvas, panel, and surface tones form the dark theme. The light theme uses a white canvas with cool gray panels. Text, muted text, border, hover, and selection each have separate roles. Use the corresponding `dark-*` or `light-*` token family together.

Success marks live connections; error marks failures. SQL keyword, string, number, and type tokens support editor syntax. Negative result bars use the keyword color. These functional colors supplement the primary accent.

**The Context Rule.** Use blue for actions, selection, and data marks; pair connection and failure colors with readable state text.

## Typography

Space Grotesk (400, 500, 600) and Space Mono (400, 700) are self-hosted through Fontsource. Controls inherit Space Grotesk; SQL, results, history statements, and code use Space Mono. CodeMirror disables ligatures.

The base is body text; navigation, controls, and table data generally use the label or data role. Secondary metadata ranges from 8–10px. Source and history view headings use the title size; query history uses a medium-weight variant, and the welcome heading is 21px. Inventory counts use 24px medium text. Avoid treating this compact hierarchy as a marketing page scale.

**The Data Rule.** Keep SQL, returned data, and code in Space Mono; use Space Grotesk for surrounding controls.

## Layout

The application fills the viewport and prevents body scrolling. A single workspace row (34px), document tab strip (35px), and bottom status bar (23px) keep the chrome slim. The source sidebar is 244px wide on desktop. Console and result toolbars are 33px and 31px high. Each content pane owns its scrolling and has a zero minimum size where flex sizing requires it.

Source rows are single-line (32px), child navigation is 28px, table rows are 26px, and column rows are 24px. The active source exposes a separate Tables disclosure that starts collapsed. Schema rows separate expanding the tree from opening a schema document.

Documents can belong to different sources and have independent close controls. The plus menu opens consoles and source views. Inactive panels remain mounted and hidden, retaining their editor, result presentation, split, and view filters. Source context appears in the breadcrumb and tab tooltips. Tabs always show their source connection name, including when every document belongs to the same source. The breadcrumb also holds sidebar visibility, connection, refresh, and theme controls.

A console with results places the editor above them, starting at a 55/45 split with a draggable 5px separator. Before results exist, the editor uses the available height and a slim idle strip replaces the result pane. Each console belongs to the source shown in its document tab. The Schema dropdown retains a choice per console and applies it before each query on supported drivers. MySQL uses the selected schema as its current database. A table document uses the full content height and switches between Data and Structure rather than showing a SQL editor above its rows.

Overview content uses a four-column inventory strip and two-column charts, followed by flat object and history lists. The database browser splits object inventory and inspector at roughly 30/70. Schema map cards live on a scrollable, zoomable canvas.

Source views use 22px top, 26px side, and 28px bottom padding, stepping down to 18px below 700px. Dashboard columns stack below 760px. The sidebar narrows to 222px below 1000px, 195px below 700px, and 172px below 480px. Smaller windows hide secondary hints and tab source labels while retaining scrollable documents and data panes.

The settings dialog is capped at the viewport and uses a persistent database catalog beside connection details. Below 480px that catalog stacks above the fields.

## Elevation & Depth

Ordinary workspace panes are flat. Tonal backgrounds, one-pixel borders, and active underlines define their boundaries. Menus and dialogs use the Retro theme’s `shadow-md` token; the settings dialog keeps its dimmed backdrop.

**The Overlay Rule.** Reserve shadows for menus and dialogs; separate working panes with tone and borders.

## Shapes

Controls, selected rows, schema cards, and dialogs use the Retro theme’s zero-radius token. Connection indicators and loading spinners remain circular.

## Components

- **Buttons:** filled blue for Run and primary actions; bordered secondary controls; transparent icon and text controls for supporting actions. Buttons transition background, text, and border color over 140ms. Disabled controls reduce opacity and stop advertising interaction.
- **Selects:** every option picker uses shadcn Select with keyboard navigation, typeahead, disabled states, and themed menus. Menus inside native dialogs portal into their owning dialog. Empty values such as All schemas remain selectable.
- **Fields:** thin neutral border, compact padding, theme-aware canvas or panel fill. Focus uses the accent outline; composite search fields also change their border. Preserve explicit labels on filters and icon-only actions.
- **Source navigation:** an expandable single-line source row carries name, dialect, state, and a separate actions menu. Child views use a branch rule, icons, and a selected background. The active source exposes a collapsed Tables group containing a searchable schema/table/column explorer with separate open actions.
- **Document tabs:** show kind, title, a source subtitle, and a separate close button; tooltips always include the source. Active documents use canvas fill, readable text, an accent icon, and a two-pixel underline. The strip scrolls horizontally. The plus menu opens a console or source view; keyboard navigation supports arrows, Home, End, and Delete.
- **Console schema selector:** an explicit Schema label and native select choose the execution schema for that console. The connection name stays visible in its tab. Changing schemas preserves SQL and clears prior results and errors. Schema setup and the query share one connection lock.
- **Table documents:** Data fills the available pane with rows or a chart. Structure shows reported column types, defaults, nullability, PK/FK markers, constraints, references, and indexes. Refresh rows, Open SQL, and Export CSV remain nearby.
- **Schema cards:** panel fill and border, tinted header, column/type rows, PK/FK markers, and an index/count footer. Cards show up to seven columns. Curved blue arrows represent declared foreign keys between visible objects; clicking a table heading opens a data preview.
- **Data grids:** virtualized rows, sticky headers, alternating tonal bands, right-aligned numeric cells, and muted null values. The results toolbar switches between table and chart and exports displayed results as CSV.
- **Charts:** simple horizontal bars with readable labels and values. Overview bars summarize inspected schemas and column types. Result charts offer label/value selectors, a zero baseline, negative bars, and a caption stating the data limits.
- **History:** flat, separated entries show completion state, time, duration, rows, SQL, and an action to load SQL into the console. Search covers SQL and error text. History is scoped to the selected source and labeled as session-only.
- **Empty and busy states:** explain the missing connection, catalog, numeric data, or query and provide a relevant next action where available. Connection and query activity use a thin progress strip, spinner, or static loading bars. Reduced-motion preferences remove button transitions and rotating animations.

## Do's and Don'ts

### Do:

- Do use the shared theme variables for surfaces, text, borders, and state.
- Do keep source context visible in navigation and workspace breadcrumbs.
- Do pair state colors with labels, icons, counts, or key markers.
- Do preserve scrolling, truncation, and focus states when adding dense content.

### Don't:

- Don't add large display typography to routine database views.
- Don't use shadows to separate ordinary workspace panes.
- Don't draw relationships or chart values that the catalog or query result does not supply.
- Don't treat session history or SQL drafts as durable storage.


## Tailwind implementation

The frontend uses Tailwind v4 with the Vite plugin and `tw:` utility prefix. `apps/dg-desktop/src/styles.css` is the sole stylesheet entry. `styles/retro.css` contains the installed tweakcn Retro theme, customized with a blue primary; `styles/theme.css` maps workbench/editor roles to the shared tokens. `styles/components.css` composes shared and stateful styles with `@apply`; standalone layout utilities live in JSX. Preflight is intentionally omitted to preserve native dialog, form, and heading behavior. Tailwind v4 targets Safari 16.4 or newer.

`components.json` configures shadcn with the existing Radix icon family. All nine option pickers use `SelectField`, composed from `components/ui/select.tsx`. Avoid introducing native select controls. The theme’s malformed light sidebar color was corrected to the card surface.

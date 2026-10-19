---
name: Rowcraft landing
description: Dark monospace marketing surface drawn from the Rowcraft desktop UI.
colors:
  blue: "#2563eb"
  selection: "#1e3a5f"
  green: "#82c9ad"
  purple: "#b6a2ed"
  bg: "#19191d"
  panel: "#232323"
  line: "#47474a"
  text: "#f3f3f1"
  muted: "#b0b0b3"
typography:
  display:
    fontFamily: "SpaceMono, ui-monospace, monospace"
    fontSize: "clamp(44px, 4.9vw, 70px)"
    fontWeight: 400
    lineHeight: 1.17
    letterSpacing: "-.04em"
  headline:
    fontFamily: "SpaceMono, ui-monospace, monospace"
    fontSize: "clamp(30px, 3.25vw, 46px)"
    fontWeight: 400
    lineHeight: 1.3
    letterSpacing: "-.035em"
  body:
    fontFamily: "SpaceMono, ui-monospace, monospace"
    fontSize: "13px"
    lineHeight: 1.8
  brand:
    fontFamily: "Space, sans-serif"
    fontSize: "25px"
    fontWeight: 600
    lineHeight: 1
    letterSpacing: "-.025em"
rounded:
  control: "0px"
components:
  button-primary:
    backgroundColor: "{colors.blue}"
    textColor: "#fff"
    rounded: "{rounded.control}"
    padding: "10px 20px"
  button-primary-hover:
    backgroundColor: "#1d4ed8"
  platform-selected:
    backgroundColor: "{colors.selection}"
    textColor: "#e1ecff"
    rounded: "{rounded.control}"
---

# Design System: Rowcraft landing

## Overview

The landing page carries Rowcraft’s desktop UI into a dark, technical marketing surface. Monospace typography, charcoal panels, square controls, and thin rules make the real workspace screenshot its visual anchor. The user confirmed this direction, including the dark palette and “Built with Rust” label. The Rust credit sits directly below the left hero headline at 18px (15px on mobile), with “Rust” in pale blue.

Scope is `apps/landing` only. Root `DESIGN.md` and `.impeccable/design.json` describe the desktop app. Implementation sources are `index.html`, `styles.css`, `main.js`, and the existing download configuration and renderer. Product context comes from root `PRODUCT.md`; the current landing code and bundled benchmark supersede its older demo/evidence descriptions.

## Colors

Cobalt identifies primary actions and the Rowcraft benchmark bar. Navy selection identifies selected controls and schema headers. Green marks connection status and SQL values; purple marks SQL keywords. Charcoal canvas and panel tones are separated by visible gray rules. Off-white and muted gray establish the reading hierarchy.

Focus uses a blue outline (`#80abff`, 2px, offset 5px). The observed peak bar is neutral gray (`#8c8c92`). Supporting blue text accents remain subordinate to primary actions.

## Typography

Space Mono 400, locally loaded with `font-display: swap`, supplies headings, prose, navigation, controls, code, and chart labels. The Rowcraft wordmark alone uses local Space Grotesk 600 under the `Space` alias. Do not restore the previous sans-serif display system.

Display and headline tokens describe desktop sizing. Feature titles use 30px; ordinary marketing copy uses 11–12px with generous line height. Small UI captions use 9–10px. At the mobile breakpoint the hero uses `clamp(34px, 8.7vw, 56px)` with 1.25 line height. Keep interface illustrations compact while preserving clear page hierarchy.

## Layout

The centered wrapper is `min(1240px, calc(100% - 96px))`. The 80px ruled header precedes a split hero, workspace screenshot, database strip, benchmark, alternating feature rows, three utility columns, open-source band, FAQ, download panel, and footer. Feature columns have an 88px desktop gap. Major sections use approximately 56–88px vertical intervals.

Breakpoints are 1100px, 850px, and 700px. Wrapper gutters reduce to 32px and then 18px. Below 850px, installation options stack and the decorative connection list disappears. Below 700px, navigation hides, hero and content grids stack, the header becomes 70px tall, and chart captions become a vertical list. The chart remains responsive SVG with larger mobile text in its viewBox. Code blocks wrap or scroll within their containers.

## Elevation & Depth

**The Flat Workspace Rule.** Use tonal surfaces and one-pixel rules to separate content. The screenshot, SQL illustration, controls, and benchmark have no decorative shadow, perspective, or pointer tilt. The prior slab sculpture and recessed screenshot backplates are removed.

The opening entrance assembles the header, headline, Rust credit, supporting panel, actions, and screenshot in reading order. Web Animations API transitions combine a short vertical movement, fade, and shallow clip reveal over 620ms, with delays capped at 380ms. Deep links and restored scroll positions skip this sequence. Below-fold sections reveal once over 560ms; the benchmark bars grow from a shared zero baseline over 700ms when their section enters view. All elements remain visible by default without scripting. Reduced-motion preferences skip or cancel these effects; hidden pages cancel active animations, and keyboard focus cancels motion on its containing element.

## Shapes

Controls and panels use square corners and straight borders. Small circular connection indicators remain functional exceptions. Inline SVG icons use thin strokes. Keep schema connections and chart grids aligned to the page’s ruled structure.

## Components

- **Actions and navigation:** square cobalt primary actions, outlined secondary source action, and muted text links. Hover changes color without lifting controls. Retain the skip link and visible keyboard focus.
- **Workspace preview:** unchanged `assets/ui.jpg` (1600 × 1001), framed by a flat toolbar and caption. The image and full-size link open the asset in a new tab. This is a screenshot, not an interactive database demo.
- **Benchmark:** keyboard-accessible Memory and CPU tabs show Rowcraft average and peak observed resource use. Memory uses a 0–200 MiB axis; CPU uses a 0–4% axis. Preserve the numeric captions, italic “Tested on Mac” label, visible limitations, disclosure, and recorded JSON. This is one 120-second connected-idle shared-machine capture with 98 samples, excluding external WebKit helpers. It does not measure complete app footprint, query speed, or startup speed.
- **Disclosures:** native `details` for compatibility, benchmark conditions, FAQ, and source instructions. Source anchors open their disclosure, including direct hash loading.
- **Installation:** `config.js` and `downloads.js` retain OS selection, native download formats, terminal method selection, command copying, and accessible feedback. Selected OS uses navy fill and a cobalt underline. Unpublished assets remain disabled and registry examples remain explicitly labeled previews. Source instructions stay available without JavaScript.
- **Release selection:** a labeled native Version select sits above the OS tabs, alongside the Release notes link. Preserve its square charcoal surface, thin gray border, monospace text (12px), and minimum control height (44px). The field grows from 220px to a maximum of 340px; the row wraps with a 16px gap on narrow screens. Muted status text below it announces the selected version, missing installers, or saved-release fallback through a polite live region.
- **Release consistency:** Latest is the default on each page load and follows GitHub’s explicit latest-release marker. Other published versions remain selectable, with prereleases labeled “Pre-release.” Version changes update installer links, installer filenames in setup commands, and the Release notes destination together. Missing assets retain disabled “Coming soon” actions and preview commands. `releases.js` refreshes the paginated catalog; the bundled snapshot in `config.js` keeps selection available when that refresh fails, with a visible saved-releases notice.
- **Assets:** existing mark, local font files, unchanged screenshot, inline SVG icons, and sanitized benchmark JSON. No remote image or font dependency is needed.

## Do's and Don'ts

- Do match the desktop’s charcoal palette, monospace rhythm, square controls, and ruled panels within this landing surface.
- Do retain the actual workspace screenshot and explicit “Built with Rust” label.
- Do preserve benchmark scope, versions, methodology, and visible helper-process limitation when editing the comparison.
- Do retain keyboard focus, native disclosures, reduced-motion support, and honest download availability.
- Don't describe idle CPU measurements as speed or general performance superiority.
- Don't add decorative sculpture, floating backplates, perspective, or screenshot tilt.
- Don't imply that the screenshot connects to live databases or that unpublished downloads are available.
- Don't apply landing changes to the desktop design documents without a separate design decision.

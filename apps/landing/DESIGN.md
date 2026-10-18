---
name: Rowcraft landing
description: Dark architectural marketing surface for an open source database workbench.
colors:
  bg: "#101113"
  panel: "#17191c"
  line: "#303236"
  text: "#efefeb"
  muted: "#a3a5ab"
  blue: "#365be3"
  green: "#a4bba0"
  purple: "#baa7de"
typography:
  display:
    fontFamily: "Space, sans-serif"
    fontSize: "clamp(62px, 6.4vw, 88px)"
    fontWeight: 400
    lineHeight: 1.02
    letterSpacing: "-.04em"
  body:
    fontFamily: "Space, sans-serif"
    fontSize: "15px"
  code:
    fontFamily: "SpaceMono, monospace"
    fontSize: "11px"
    lineHeight: 1.9
rounded:
  button: "4px"
  illustration: "5px"
  workbench: "9px"
components:
  button-primary:
    backgroundColor: "{colors.blue}"
    textColor: "#fff"
    rounded: "{rounded.button}"
    padding: "0 24px"
    height: "54px"
---

# Design System: Rowcraft landing

## Overview

The implemented surface extends Rowcraft into a spacious, dark architectural landing page inspired by the user-pinned `samples/arccheo.framer.ai/index.html`. Large editorial type, thin rules, restrained cobalt actions, an existing white brand mark, and a compact interactive workbench carry the page. The desktop app has its own system; these rules apply to `apps/landing` only.

Source of truth: `index.html`, `styles.css`, `main.js`, and `config.js`, interpreted with root `PRODUCT.md` and `.impeccable/surfaces/landing.md`. This document records code inspection. No browser preview or screenshots were available: the sandbox denied the preview server. Rendered layout, contrast, overflow, focus visibility, browser downloads, and clipboard behavior remain unverified visually or end to end.

## Colors

The frontmatter mirrors the eight `:root` CSS color variables. Near-black canvas and charcoal surfaces are separated by quiet gray rules. Off-white headlines and muted gray prose form the main hierarchy. Cobalt signals primary actions; pale green and purple distinguish SQL values and keywords.

Additional implemented treatments include the navy download panel (`#202b46`), light download action (`#e9edf7`, ink `#17233e`), open-source band (`#191b20`), and active explorer rows (`#28324a`). The global keyboard focus outline is `2px solid #99acff`, offset `5px`.

## Typography

Local Space Grotesk files use the CSS family alias `Space` at weights 400/500/600/700; Space Mono 400 uses `SpaceMono`. All fonts use `font-display: swap`. The hero’s final word keeps Space and uses a pale blue accent. Database wordmarks also use deliberate text treatments, including Georgia for SQLite.

Hero display is 62–88px, regular weight, tight tracking and 1.02 line height. Section headings use `clamp(40px,4.6vw,64px)`, 1.05 line height, and the same tracking; feature headings are 36px. Marketing paragraphs are typically 12–14px with 1.7–1.8 line height. Uppercase section labels are 9px with 1.5px tracking. The demo deliberately uses dense 8–11px labels and monospace results; these are product illustration scale, not the general prose scale.

## Layout

The standard centered wrapper is `min(1240px, calc(100% - 112px))`. At 1500px and above it becomes `min(1320px, calc(100% - 160px))`. The header is 108px tall. A split hero precedes the full-width workbench, database strip, alternating two-column feature rows, three utility columns, full-bleed open-source band, split FAQ, navy download panel, and ruled footer. Major vertical intervals range roughly 58–115px; dividers provide structure without surrounding every section in cards.

At 1050px and below, wrapper gutters become 32px each; grids tighten, the explorer narrows from 205px to 170px, schema tables shrink, and the intro prose moves below its heading. At 700px and below, gutters become 20px; navigation is hidden while the download action stays visible. Hero, features, open-source, FAQ, utilities, and download content stack. The slab sculpture disappears. Hero type uses `clamp(46px,10.5vw,72px)`; the download heading is 36px. Feature schema art follows its copy. Database marks wrap; the footer becomes two columns.

The mobile workbench replaces the sidebar tree with three horizontal table buttons and reduces chrome/type. Results retain horizontal scrolling and nowrap cells. Build commands have an overflow container. These are implemented breakpoint intentions; browser fitting has not been confirmed.

## Elevation & Depth

Most surfaces are flat, relying on tonal changes and one-pixel borders. The demo alone has an ambient `0 28px 80px -45px #000` shadow. A CSS-built four-slab sculpture uses perspective, rotated planes, thin blue borders, a restrained gradient on the top slab, and `2px 9px 18px #0004` shadows. Its top plane lifts on hover. The query illustration rotates −2 degrees; schema tables use opposing vertical offsets.

The hero title enters over 1.2 seconds with a small upward translation and clip reveal. Buttons transition background and lift 2px on hover; other state changes use short color/border/icon transitions. `prefers-reduced-motion: reduce` disables animations/transitions and smooth scrolling.

## Shapes

Compact radii preserve the precise workbench character: 4px buttons, 5px feature illustrations, 8px download panel, 9px desktop workbench (6px mobile), 2–3px badges and table controls. Thin rectangular grids dominate. Tiny circular status indicators and window controls provide the recurring rounded detail. Icons are inline SVG symbols with mostly 1.5px strokes; they are not an external icon dependency.

## Components

- **Actions and navigation:** cobalt primary buttons with white type; slate secondary source action; text links shift to pale blue on hover. The final download button is light against navy. Header navigation uses muted text and white hover. Anchors use smooth scrolling with top clearance. A fixed skip link appears on focus.
- **Interactive workbench:** customers/orders/products buttons populate an editable SQL textarea and local result rows; active buttons expose `aria-pressed`. Results and Structure tabs expose selection, use roving tab stops, and accept arrows/Home/End. Structure shows column/type/nullability. Run query disables during a 380ms simulated delay, announces status, and returns to Results; Cmd/Ctrl+Enter works within the demo. The read-only sample query runner supports SELECT projections, WHERE comparisons with AND, ORDER BY, and LIMIT. Unsupported statements and invalid columns show an inline error. It does not connect to databases. Demo links temporarily accent the border and focus Run query. Export creates CSV from the last query result or the current schema view.
- **Disclosures:** native `details` elements hold database requirements, FAQ answers, and source build instructions. FAQ plus icons rotate 45 degrees when open. Relevant anchors open the compatibility/build disclosure; direct `#source-guide` loading also opens it.
- **Download/source configuration:** `config.js` supplies public HTTPS destinations and package IDs. `downloads.js` renders each OS and format, disabled unpublished downloads, real or preview package commands, and clipboard feedback. See the installation section below. `main.js` configures repository/source links and opens source disclosures.
- **Assets:** `assets/mark.svg` supplies favicon/header/footer/open-source branding. Five local WOFF2 files supply the two font families. Database symbols, feature graphics, sculpture, and UI icons use HTML/CSS/inline SVG; no remote image or font service is required.

## Do's and Don'ts

- Do preserve spacious marketing hierarchy around the dense, clearly labeled sample workbench.
- Do retain keyboard focus styles, tab semantics, native disclosures, and reduced-motion behavior when extending components.
- Do keep source build instructions visible as the fallback until actual repository/releases are configured.
- Do verify desktop/mobile rendering and functional browser behavior before describing the page as visually or end-to-end tested.
- Don't imply that the browser demo connects to a live database or supports SQL beyond its documented SELECT subset.
- Don't invent popularity, benchmarks, license details, customer proof, or published binary availability.
- Don't apply these landing-specific tokens to the desktop app without a separate design decision.

## Scroll transitions

Sections below the initial viewport reveal once with a 28px upward movement and fade over 750ms, using exponential ease-out. IntersectionObserver triggers the browser Web Animations API without scroll handlers or scroll interception. Initial viewport content and restored scroll positions remain visible. Reduced-motion preferences skip reveals and cancel active animations when changed; keyboard focus cancels an active reveal on its containing section. Unsupported browsers and disabled JavaScript retain visible content. The header GitHub link uses `repositoryUrl` when configured and otherwise points to the source section.

## Hero refinement and platform installation

The hero keeps the existing copy, mark, Space typography, and cobalt accent. Display type is capped at 88px with same-family blue emphasis on “data”. The two-column hero aligns centrally; its sculpture has a 190px layout area and a 24px gap before body copy, reserving space for transformed layers. Decorative coordinate axes and the corner slogan are removed. Mobile type is 46–72px, with actions that wrap and a 44px header action.

The navy download panel uses a compact 36–54px heading, short introductory copy, and a three-button OS picker. Explicit transparent button backgrounds prevent browser-default dark rectangles. The selected platform uses a light surface with dark ink. The two content columns use a 1:1.1 ratio and stack below 850px. Native installer actions are light buttons; unpublished assets are disabled “Coming soon” buttons with accessible names.

Terminal instructions use a dark inset surface with one border, a header identifying the shell and native method select, wrapped monospace commands, and a copy footer. Unpublished registry IDs show labeled command previews with explicit owner/publisher placeholders and a comment stating that publication is required. Copy reads “Copy example” in that state. Configured IDs show live installation commands. Linux commands install local downloaded files. Inline clipboard feedback uses a live region. Source instructions remain available without JavaScript.

Configuration: `config.js`. Behavior: `downloads.js`. Publishing: `docs/package-distribution.md`. The existing responsive layout, local fonts, focus outlines, and reduced-motion support are preserved. Code checks passed. Browser testing was explicitly excluded by the user for this refinement; the updated rendering has not been visually verified.

## Desktop screenshot and depth

The interactive browser demo is replaced by the supplied `ui.jpg`, copied unchanged into `assets/ui.jpg` (1600 × 1001). Its image and caption link open the full-size asset. Related links, metadata, and FAQ text now describe a screenshot preview; the old SQL runner is not loaded.

A short perspective entrance settles to no transform. Pointer motion rotates only the two recessed backplates, preserving screenshot text on a flat image plane. A separate rounded, clipped image wrapper hides square corner seams while the frame’s depth remains outside the clip. Keyboard focus, reduced-motion preferences, coarse pointers, page blur, and page visibility changes reset motion. Mobile keeps a flat preview. No image generation, upscaling, or alteration of screenshot text was applied; sharper Retina detail requires a higher-resolution original capture.

JavaScript syntax and the retained SQL utility tests passed. Browser testing was excluded by user instruction.

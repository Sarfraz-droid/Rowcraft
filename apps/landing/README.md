# Rowcraft landing page

Standalone static marketing site with PostHog analytics.

## Preview

From the repository root, run:

```sh
make landing
```

Open http://127.0.0.1:4321. Press Ctrl+C to stop. Requires Python 3; no dependency installation or build step is needed. To use another port, run `make landing LANDING_PORT=8080`.

Run `make landing-check` from the repository root to check JavaScript syntax and run the sample-query tests (requires Node/npm).

From this directory, `npm run dev` generates the public analytics configuration and starts the preview. Requires Node 20.12+ and Python 3. Run `npm run build` before deploying to any static host, then publish `dist/`. The build includes the generated `runtime-config.js` and all browser assets.

For Vercel, set the project's Root Directory to `apps/landing`. The checked-in `vercel.json` selects `npm run build` and the `dist` Output Directory.

## Configure downloads

The Version picker defaults to GitHub's latest stable release. It lists the other
published releases, including pre-releases with a label. Choosing a version updates
all platform downloads, release notes, and installer commands together. A release
without matching installer assets shows unavailable downloads instead of guessed
URLs. Reloading the page selects Latest again.

`config.js` contains a saved public release snapshot for offline or rate-limited
clients. On load, `releases.js` refreshes the catalog from the repository's GitHub
Releases API, follows pagination, and respects GitHub's explicit latest marker.
If fetching fails, the saved list remains usable and the page identifies it.


The download panel links to the published [v0.0.1 release](https://github.com/Sarfraz-droid/Rowcraft/releases/tag/v0.0.1). Update `repositoryUrl` and verified HTTPS installer URLs in `config.js` for future releases:

- `downloads.macos.arm64` / `x64`: Apple Silicon / Intel DMGs.
- `downloads.windows.exe` / `msi`: x64 installers.
- `downloads.linux.appimage` / `deb` / `rpm`: x64 packages.

The main tabs select macOS, Windows, or Linux, initially matching the visitor's desktop platform. Within macOS, choose Apple Silicon or Intel; within Linux, choose DEB, RPM, or AppImage; Windows offers EXE. Each OS remembers its selected installer when switching tabs. Arrow keys, Home, and End work within both tab levels. Missing or non-HTTPS URLs show disabled “Coming soon” buttons. Older string URLs per platform remain supported. Without JavaScript, source build instructions remain available.

Set `packages.homebrew` to a published full cask token (`owner/tap/rowcraft`), `packages.winget` to the approved package ID, and `packages.chocolatey` to the approved package name. Empty or invalid IDs show clearly labeled command previews with a comment explaining that publishing is required. Homebrew and WinGet previews include explicit owner/publisher placeholders; the copy button says “Copy example”. Configured IDs show the actual command. These commands install the desktop app; the headless `rowcraft` CLI is a separate build.

Installer tabs appear only in the Setup commands column and switch its instructions, each with its own copy button. The downloads column always lists every installer for the selected OS. Both macOS tabs also include quarantine removal instructions. Unpublished registry previews are omitted from these installer tabs. Visitors download a file into Downloads, then open the matching macOS DMG, run the Windows EXE from PowerShell, or install/run the Linux DEB, RPM, or AppImage. Commands use the configured asset filename, restricted to shell-safe characters; no renaming is needed. MSI remains unavailable because v0.0.1 does not publish one.

The macOS “Allow opening” option provides `xattr -dr com.apple.quarantine "/Applications/Rowcraft.app"` followed by `open`. It explains that the app must first be copied into Applications and that quarantine should only be removed for a trusted download blocked by macOS. Linux guidance covers AppImage lsof/FUSE requirements and the extraction fallback. These instructions do not set up an APT/DNF repository or automatic updates.

See [package-manager publishing](../../docs/package-distribution.md) for the release, Homebrew, WinGet, Chocolatey, and Linux setup.

## Desktop preview and motion

The landing page uses `assets/ui.jpg`, copied unchanged from the supplied root `ui.jpg`. It replaces the interactive sample-data workbench. Both the image and “View full size” link open the original screenshot in a new tab.

A brief perspective entrance introduces the screenshot, with two recessed backplates for depth. Once the entrance finishes, the image has no transform. On devices with a fine pointer, only the recessed layers follow the pointer with rotation capped at 2.5 degrees vertically and 3 degrees horizontally. The screenshot stays on a flat, untransformed image layer to preserve text clarity. A separate rounded clip contains the image corners; depth layers remain outside it. A single requestAnimationFrame loop smooths the movement and stops at rest. Pointer exit settles it flat; keyboard focus, window blur, and reduced-motion changes reset it immediately. Touch devices keep a flat preview; mobile disables the entrance animation. Existing section reveals remain in place.

PostHog loads asynchronously when configured. Fonts are served locally. The legacy `demo-query.js` utility and its tests are retained but are no longer loaded by the landing page.

## Checks

`npm run check` validates JavaScript syntax. `npm test` checks analytics event ordering and payloads, release selection, benchmark tabs, and the sample SQL utility. Browser testing is excluded at the user's request; motion and layout have not been visually verified.


## Analytics

Set `POSTHOG_PROJECT_TOKEN` (public `phc_` project token) and `POSTHOG_HOST` in
`.env` or the deployment build environment, then run `npm run build`.
Only those two public values are emitted into ignored `runtime-config.js`;
never use a personal API key. `npm run dev` runs this build automatically.
When using `make landing`, generate the configuration first with
`npm --prefix apps/landing run build`. Without configuration, analytics stays disabled.

[Rowcraft Landing Performance](https://us.posthog.com/project/674086/dashboard/2230246)
contains daily page views, unique visitors and sessions, installer clicks, CTA and
download conversion trends, a visit → download section → installer click funnel,
OS and CTA-placement breakdowns, and referrers. The default is the last 30 days,
using the project's UTC timezone. Filters require `app = rowcraft-landing` and
`environment = production`; localhost traffic is tagged `development`.
These are project-specific definitions, not approved Data Catalog metrics.

Click rate is the percentage of unique page visitors who click a download CTA
within one day. Download rate is the percentage who click a published installer
link within one day. Rates use ordered funnels, attributed to the day of entry;
the most recent day can still gain conversions. The detailed funnel also measures
visibility of the download section. Visitors can scroll or deep-link there without
clicking a CTA or changing OS. A download event confirms a link click, not a
completed transfer or an installed app. Unique visitors represent browser identities.

| Event | Trigger / properties |
| --- | --- |
| `$pageview` / `$pageleave` | One page view on initialization, page exit through the SDK |
| `download_cta_clicked` | Download anchor click; `surface`: header, hero, content, footer |
| `download_section_viewed` | Download section at least 10% visible, once per page load |
| `installer_download_started` | Published installer link click; `platform`, `installer_type`, `release_version` |
| `download_platform_selected` | Explicit OS tab selection; `platform` |
| `download_installer_selected` | Setup installer tab selection; `platform`, `installer_type` |
| `release_version_selected` | Release picker change; `release_version`, `is_prerelease` |
| `installation_command_copied` | Successful clipboard copy; `platform`, `setup_method`, `is_preview` |
| `source_build_instructions_copied` | Successful source build clipboard copy |
| `repository_link_clicked` | Repository link click; `surface` |
| `source_guide_opened` / `faq_opened` | Expanded details; stable `item_index` |

Custom events never send command text or form values. Autocapture is disabled;
existing exception and log capture remain enabled. SDK loading queues up to 100
actions and emits the page view before draining them, so fast interactions preserve
funnel ordering. Installer clicks use sendBeacon without delaying navigation.
Blocked analytics does not prevent downloads or other page actions.

SDK reference: [PostHog JavaScript usage](https://posthog.com/docs/libraries/js/usage).

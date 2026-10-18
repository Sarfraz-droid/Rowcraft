# Rowcraft landing page

Standalone, dependency-free marketing site. The desktop app is unchanged.

## Preview

From the repository root, run:

```sh
make landing
```

Open http://127.0.0.1:4321. Press Ctrl+C to stop. Requires Python 3; no dependency installation or build step is needed. To use another port, run `make landing LANDING_PORT=8080`.

Run `make landing-check` from the repository root to check JavaScript syntax and run the sample-query tests (requires Node/npm).

From this directory, `npm run dev` also starts the preview. Any static host can serve this directory directly.

## Configure downloads

Set `repositoryUrl` and verified HTTPS installer URLs in `config.js` before publishing:

- `downloads.macos.arm64` / `x64`: Apple Silicon / Intel DMGs.
- `downloads.windows.exe` / `msi`: x64 installers.
- `downloads.linux.appimage` / `deb` / `rpm`: x64 packages.

The OS picker initially selects the visitor's desktop platform and allows switching to every platform. Missing or non-HTTPS URLs show disabled “Coming soon” buttons. Older string URLs per platform remain supported. Without JavaScript, source build instructions remain available.

Set `packages.homebrew` to a published full cask token (`owner/tap/rowcraft`), `packages.winget` to the approved package ID, and `packages.chocolatey` to the approved package name. Empty or invalid IDs show clearly labeled command previews with a comment explaining that publishing is required. Homebrew and WinGet previews include explicit owner/publisher placeholders; the copy button says “Copy example”. Configured IDs show the actual command. These commands install the desktop app; the headless `dg` CLI is a separate build.

Linux instructions install local downloaded files using APT/DNF, or run an AppImage. Visitors are told to rename the file to the exact name used by the command. This does not set up an APT/DNF repository or automatic updates.

See [package-manager publishing](../../docs/package-distribution.md) for the release, Homebrew, WinGet, Chocolatey, and Linux setup.

## Desktop preview and motion

The landing page uses `assets/ui.jpg`, copied unchanged from the supplied root `ui.jpg`. It replaces the interactive sample-data workbench. Both the image and “View full size” link open the original screenshot in a new tab.

A brief perspective entrance introduces the screenshot, with two recessed backplates for depth. Once the entrance finishes, the image has no transform. On devices with a fine pointer, only the recessed layers follow the pointer with rotation capped at 2.5 degrees vertically and 3 degrees horizontally. The screenshot stays on a flat, untransformed image layer to preserve text clarity. A separate rounded clip contains the image corners; depth layers remain outside it. A single requestAnimationFrame loop smooths the movement and stops at rest. Pointer exit settles it flat; keyboard focus, window blur, and reduced-motion changes reset it immediately. Touch devices keep a flat preview; mobile disables the entrance animation. Existing section reveals remain in place.

No analytics, third-party scripts, external font requests, or animation dependencies are included. The legacy `demo-query.js` utility and its tests are retained but are no longer loaded by the landing page.

## Checks

`npm run check` validates JavaScript syntax. `npm test` checks the retained sample SQL utility. Browser testing is excluded at the user's request; motion and layout have not been visually verified.

#!/usr/bin/env bash
# Releases Rowcraft: bumps the version, pushes main and a tag, waits for the
# "Desktop installers" workflow, then publishes the release as the latest one
# so installed apps pick up the update (docs/installers.md).
#
#   scripts/release.sh 0.0.10           # the whole release
#   scripts/release.sh 0.0.10 --finish  # resume after the tag is pushed: wait, publish, verify
#
# ALLOW_DIRTY=1 includes uncommitted changes in the release commit instead of refusing.
set -euo pipefail

REPO=Sarfraz-droid/Rowcraft
WORKFLOW=installers.yml
FEED="https://github.com/$REPO/releases/latest/download/latest.json"
# Files that carry the app version; keep in sync with docs/installers.md.
CONF=apps/rowcraft-desktop/src-tauri/tauri.conf.json
PKG=apps/rowcraft-desktop/package.json

die() { echo "release: $*" >&2; exit 1; }
step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

version=${1:-}
mode=${2:-}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "usage: $0 <x.y.z> [--finish]"
[[ -z $mode || $mode == --finish ]] || die "unknown option $mode"
tag="v$version"
cd "$(git rev-parse --show-toplevel)"

command -v gh >/dev/null || die "install the GitHub CLI (brew install gh)"
gh auth status -h github.com >/dev/null 2>&1 || die "run: gh auth login -h github.com -p ssh -w"

current=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$CONF")

start() {
  step "Checking $current -> $version"
  [[ $(printf '%s\n%s\n' "$current" "$version" | sort -V | tail -1) == "$version" && $current != "$version" ]] \
    || die "$version is not newer than the current version $current"
  [[ $(git branch --show-current) == main ]] || die "switch to main first"
  git fetch -q origin main --tags
  [[ $(git rev-parse HEAD) == $(git rev-parse origin/main) ]] || die "main differs from origin/main; pull or push first"
  if [[ -n $(git status --porcelain) && -z ${ALLOW_DIRTY:-} ]]; then
    git status --short
    die "uncommitted changes; commit them, or rerun with ALLOW_DIRTY=1 to include them in the release commit"
  fi
  git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "tag $tag already exists locally"
  [[ -z $(git ls-remote --tags origin "$tag") ]] || die "tag $tag already exists on GitHub"

  step "Bumping the version to $version"
  perl -pi -e "s/^  \"version\": \"\Q$current\E\",/  \"version\": \"$version\",/" "$CONF" "$PKG"
  perl -pi -e "s/^version = \"\Q$current\E\"/version = \"$version\"/ if 1..10" Cargo.toml
  perl -pi -e "s/\`v\Q$current\E\` for the current version/\`$tag\` for the current version/" docs/installers.md
  for f in "$CONF" "$PKG" Cargo.toml; do grep -q "\"$version\"" "$f" || die "could not bump $f"; done
  cargo metadata --format-version 1 >/dev/null # refreshes the workspace versions in Cargo.lock

  step "Committing and pushing main"
  git add -A
  git commit -q -m "release: $tag"
  git push -q origin main
  step "Tagging $tag (starts the installer builds)"
  git tag -a "$tag" -m "Rowcraft $version"
  git push -q origin "$tag"
}

finish() {
  step "Waiting for the $WORKFLOW run of $tag"
  run=""
  for _ in $(seq 60); do
    run=$(gh run list -R "$REPO" --workflow "$WORKFLOW" --branch "$tag" --limit 1 --json databaseId -q '.[0].databaseId')
    [[ -n $run ]] && break
    sleep 5
  done
  [[ -n $run ]] || die "no $WORKFLOW run started for $tag"
  echo "https://github.com/$REPO/actions/runs/$run"
  gh run watch "$run" -R "$REPO" --exit-status --interval 60 >/dev/null \
    || die "the installer build failed; see https://github.com/$REPO/actions/runs/$run (fix, then rerun the failed jobs and: make release-finish VERSION=$version)"

  step "Checking the release assets"
  assets=$(gh release view "$tag" -R "$REPO" --json assets -q '.assets[].name')
  for want in latest.json "_aarch64.dmg" "_x64.dmg" "_x64-setup.exe" "_amd64.AppImage" "_amd64.deb" ".rpm" \
    "_aarch64.app.tar.gz.sig" "_x64.app.tar.gz.sig" "_x64-setup.exe.sig" "_amd64.AppImage.sig"; do
    grep -qF -- "$want" <<<"$assets" || die "release $tag is missing $want"
  done

  step "Publishing $tag as the latest release"
  gh release edit "$tag" -R "$REPO" --draft=false --latest >/dev/null

  step "Checking the update feed"
  for _ in $(seq 12); do
    served=$(curl -fsL "$FEED" | sed -n 's/^  "version": "\(.*\)",$/\1/p' || true)
    [[ $served == "$version" ]] && break
    sleep 10
  done
  [[ $served == "$version" ]] || die "$FEED serves ${served:-nothing}, not $version"
  echo "Rowcraft $version is live: https://github.com/$REPO/releases/tag/$tag"
}

[[ $mode == --finish ]] || start
finish

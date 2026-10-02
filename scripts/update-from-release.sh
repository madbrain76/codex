#!/usr/bin/env bash
# Merge an official stable Codex release into this fork branch without rebasing
# or force-pushing local work. The caller publishes with publish-github.sh.
set -euo pipefail

repository_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repository_dir"

if ! git diff --quiet || ! git diff --cached --quiet; then
    printf '%s\n' 'Refusing to update: commit or stash working-tree changes first.' >&2
    exit 1
fi

upstream_remote="${CODEX_UPSTREAM_REMOTE:-openai}"
if ! git remote get-url "$upstream_remote" >/dev/null 2>&1; then
    printf 'Missing upstream remote %q. Add https://github.com/openai/codex.git first.\n' "$upstream_remote" >&2
    exit 1
fi

git fetch "$upstream_remote" --tags --prune

tag="${1:-}"
if [[ -z "$tag" ]]; then
    release_json="$(curl --silent --show-error --fail https://api.github.com/repos/openai/codex/releases/latest)"
    tag="$(jq -r '.tag_name // empty' <<<"$release_json")"
    prerelease="$(jq -r '.prerelease // true' <<<"$release_json")"
    if [[ -z "$tag" || "$prerelease" != false ]]; then
        printf '%s\n' 'Could not resolve an official stable release tag.' >&2
        exit 1
    fi
fi

git rev-parse --verify "refs/tags/$tag^{commit}" >/dev/null
if git merge-base --is-ancestor "$tag" HEAD; then
    printf 'Already contains official release %s.\n' "$tag"
    exit 0
fi

git merge --no-ff "$tag" -m "Merge upstream release $tag"
printf 'Merged %s. Run scripts/publish-github.sh to trigger the fork build.\n' "$tag"

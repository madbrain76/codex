#!/usr/bin/env bash
# Configure a token-free HTTPS remote and push reviewed Codex commits.
# Required: GITHUB_TOKEN (or GITHUB_CODEX_TOKEN) and CODEX_GITHUB_REPOSITORY.
# The repository value is owner/name, for example madbrain76/codex.
set -euo pipefail

repository_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
codex_repository="${CODEX_GITHUB_REPOSITORY:?Set CODEX_GITHUB_REPOSITORY to owner/name}"
github_token="${GITHUB_TOKEN:-${GITHUB_CODEX_TOKEN:-}}"
: "${github_token:?Set GITHUB_TOKEN or GITHUB_CODEX_TOKEN to a GitHub fine-grained token with Contents: Read and write}"
export GITHUB_TOKEN="$github_token"

askpass_file="$(mktemp)"
chmod 700 "$askpass_file"
trap 'rm -f "$askpass_file"' EXIT
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'case "$1" in' \
  '  *Username*) printf "%s\\n" x-access-token ;;' \
  '  *) printf "%s\\n" "$GITHUB_TOKEN" ;;' \
  'esac' > "$askpass_file"

export GIT_ASKPASS="$askpass_file"
export GIT_TERMINAL_PROMPT=0

push_repository() {
  local directory="$1"
  local repository="$2"
  local remote="$3"
  local branch
  local url="https://github.com/${repository}.git"

  git -C "$directory" diff --check
  if [[ -n "$(git -C "$directory" status --porcelain)" ]]; then
    printf 'Refusing to push %s: commit or stash its working-tree changes first.\n' "$directory" >&2
    return 1
  fi

  branch="$(git -C "$directory" branch --show-current)"
  if [[ -z "$branch" ]]; then
    printf 'Refusing to push %s: HEAD is detached.\n' "$directory" >&2
    return 1
  fi

  if git -C "$directory" remote get-url "$remote" >/dev/null 2>&1; then
    git -C "$directory" remote set-url "$remote" "$url"
  else
    git -C "$directory" remote add "$remote" "$url"
  fi

  git -C "$directory" push --set-upstream "$remote" "${branch}:${branch}"
}

push_repository "$repository_dir" "$codex_repository" origin

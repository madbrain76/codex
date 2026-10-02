#!/usr/bin/env sh
# Install the latest Linux x86_64 Codex package published by this fork.
# Mirrors the official standalone installer layout so the CLI, its shared
# background server, and code mode all find their companion binaries.
set -eu

repository="madbrain76/codex"
target="x86_64-unknown-linux-gnu"
asset="codex-package-${target}.tar.gz"
install_dir="${CODEX_INSTALL_DIR:-$HOME/.local/bin}"
codex_home="${CODEX_HOME:-$HOME/.codex}"
releases_dir="$codex_home/packages/standalone/releases"
download_url="https://github.com/${repository}/releases/latest/download/${asset}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) ;;
  *)
    echo "This fork installer currently supports Linux x86_64 only." >&2
    echo "Build this fork from source for your platform: https://github.com/${repository}" >&2
    exit 1
    ;;
esac

for command in curl tar mktemp ln mkdir rmdir sed; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Required command not found: $command" >&2
    exit 1
  fi
done

temporary_dir="$(mktemp -d)"
trap 'rm -rf "$temporary_dir"' EXIT HUP INT TERM

printf '%s\n' "Downloading the latest build from ${repository}..."
curl --fail --location --silent --show-error --retry 3 \
  --output "${temporary_dir}/${asset}" "$download_url"

extract_dir="${temporary_dir}/extract"
mkdir -p "$extract_dir"
tar --extract --gzip --file "${temporary_dir}/${asset}" --directory "$extract_dir"

if [ ! -f "${extract_dir}/codex-package.json" ] || [ ! -f "${extract_dir}/bin/codex" ]; then
  echo "Downloaded archive is not a Codex package." >&2
  exit 1
fi

version="$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "${extract_dir}/codex-package.json" | head -n 1)"
case "$version" in
  *[!0-9A-Za-z.-]*|"")
    echo "Package manifest has an unusable version: ${version}" >&2
    exit 1
    ;;
esac

release_name="${version}-${target}"
release_dir="${releases_dir}/${release_name}"
mkdir -p "$releases_dir"

if [ -e "$release_dir" ]; then
  # Only ever replace a directory we previously installed under our own
  # releases root; anything else is a hard error.
  case "$release_dir" in
    "${releases_dir}/"*) rm -rf "$release_dir" ;;
    *)
      echo "Refusing to replace unrelated path: $release_dir" >&2
      exit 1
      ;;
  esac
fi
mv "$extract_dir" "$release_dir"
ln -sf bin/codex "${release_dir}/codex"

mkdir -p "$install_dir"
link_tmp="${install_dir}/.codex.$$"
ln -s "${release_dir}/bin/codex" "$link_tmp"
mv -f "$link_tmp" "${install_dir}/codex"

printf '%s\n' "Installed Codex ${version} into ${release_dir}"
printf '%s\n' "Linked ${install_dir}/codex"
printf '%s\n' "Ensure ${install_dir} is on PATH, then run: codex"

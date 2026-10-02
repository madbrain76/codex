#!/usr/bin/env sh
# Install the latest Linux x86_64 build published by this fork.
set -eu

repository="madbrain76/codex"
asset="codex-x86_64-unknown-linux-gnu.tar.gz"
install_dir="${CODEX_INSTALL_DIR:-$HOME/.local/bin}"
download_url="https://github.com/${repository}/releases/latest/download/${asset}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) ;;
  *)
    echo "This fork installer currently supports Linux x86_64 only." >&2
    echo "Build this fork from source for your platform: https://github.com/${repository}" >&2
    exit 1
    ;;
esac

for command in curl tar mktemp install; do
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
tar --extract --gzip --file "${temporary_dir}/${asset}" --directory "$temporary_dir"

if [ ! -f "${temporary_dir}/codex" ]; then
  echo "Downloaded archive did not contain the Codex binary." >&2
  exit 1
fi

mkdir -p "$install_dir"
install -m 755 "${temporary_dir}/codex" "${install_dir}/codex"
printf '%s\n' "Installed ${install_dir}/codex"
printf '%s\n' "Ensure ${install_dir} is on PATH, then run: codex"

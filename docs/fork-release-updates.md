# Updating this fork from official Codex releases

This fork follows OpenAI's **stable release tags**, not `openai/main`. That
keeps the installed and built CLI on an official version such as `0.160.0`.

From the Codex checkout:

```bash
scripts/update-from-release.sh
CODEX_GITHUB_REPOSITORY=madbrain76/codex scripts/publish-github.sh
```

The updater obtains the current stable tag from OpenAI's release API, fetches
that tag from the `openai` remote, and creates a normal merge commit. It never
rebases or force-pushes the fork. To select a known tag explicitly, pass it:

```bash
scripts/update-from-release.sh rust-v0.160.0
```

`publish-github.sh` pushes only the clean Codex fork branch. The `fork-build`
workflow checks formatting, runs deterministic CLI/core unit tests, builds the
release CLI, and uploads the Linux artifact. `cdx` remains local and is never
pushed by this procedure.

For a local installed binary, point `CODEX_BIN` at the GitHub-built artifact
after downloading it, or install it to your preferred executable location.
The next `cdx` launch uses that binary automatically; aliases, local route
maps, sessions, and `CODEX_HOME` are untouched.

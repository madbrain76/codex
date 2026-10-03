<p align="center"><strong>Codex CLI</strong> is a coding agent from OpenAI that runs locally on your computer.
<p align="center">
  <img src="https://github.com/openai/codex/blob/main/.github/codex-cli-splash.png" alt="Codex CLI splash" width="80%" />
</p>
</br>
If you want Codex in your code editor (VS Code, Cursor, Windsurf), <a href="https://developers.openai.com/codex/ide">install in your IDE.</a>
</br>If you want the desktop app experience, run <code>codex app</code> or visit <a href="https://chatgpt.com/codex?app-landing-page=true">the Codex App page</a>.
</br>If you are looking for the <em>cloud-based agent</em> from OpenAI, <strong>Codex Web</strong>, go to <a href="https://chatgpt.com/codex">chatgpt.com/codex</a>.</p>

---

## Quickstart

### Installing and running Codex CLI

Install this fork's latest passing Linux x86_64 build with:

```shell
curl -fsSL https://raw.githubusercontent.com/madbrain76/codex/main/scripts/install/install-fork.sh | sh
codex
```

The installer downloads the release asset published by this fork's
[GitHub Actions workflow](../../actions/workflows/fork-build.yml) and installs
the complete managed package under `~/.codex`, then links `codex` into
`~/.local/bin` (or `CODEX_INSTALL_DIR`). This includes the companion binaries
required by the background server and code mode. To use it through the local
launcher without installing it globally:

```shell
CODEX_BIN=/path/to/codex cdx <alias>
```

The workflow publishes Linux and Windows x86_64 packages. For another platform,
[build from source](./docs/install.md) in this fork rather than using an
upstream installer.

Windows x86_64 packages are installed with PowerShell:

```powershell
irm https://raw.githubusercontent.com/madbrain76/codex/main/scripts/install/install-fork.ps1 | iex
```

<details>
<summary>Install unmodified upstream Codex instead</summary>

These commands deliberately install OpenAI's official, unmodified build--not
this fork:

```shell
curl -fsSL https://chatgpt.com/codex/install.sh | sh
```

```powershell
irm https://chatgpt.com/codex/install.ps1 | iex
```

</details>

### Using Codex with your ChatGPT plan

Run `codex` and select **Sign in with ChatGPT**. We recommend signing into your ChatGPT account to use Codex as part of your Plus, Pro, Business, Edu, or Enterprise plan. [Learn more about what's included in your ChatGPT plan](https://help.openai.com/en/articles/11369540-codex-in-chatgpt).

You can also use Codex with an API key, but this requires [additional setup](https://developers.openai.com/codex/auth#sign-in-with-an-api-key).

## This fork

This fork tracks OpenAI's stable Codex release tags rather than `openai/main`.
Its packages append `-madbrain` to the matching upstream version (for example,
`0.160.0-madbrain`) so `codex --version` identifies the custom build. To bring
the fork forward and publish the resulting branch from the Codex checkout:

```bash
scripts/update-from-release.sh
CODEX_GITHUB_REPOSITORY=madbrain76/codex scripts/publish-github.sh
```

The updater selects OpenAI's latest stable release, fetches its tag, and
creates a normal merge commit; it never rebases or force-pushes. To use a
specific release instead, supply its tag:

```bash
scripts/update-from-release.sh rust-v0.160.0
```

Publishing pushes only this Codex fork. GitHub's `fork-build` workflow checks
formatting, runs deterministic CLI and core tests, builds a complete standalone
package, and uploads its Linux artifact. Download that artifact and point
`CODEX_BIN` at its `bin/codex` executable to use it with the local launcher.

The companion `cdx` checkout is intentionally local-only and is never pushed
by this procedure. Its aliases can generate the Codex route map with `cdx
routes`; start an alias with `cdx <alias>`, or resume one with `cdx <alias>
resume`. An active Codex turn also accepts a new submitted message as immediate
steering for that turn.

## Docs

- [**Codex Documentation**](https://developers.openai.com/codex)
- [**Contributing**](./docs/contributing.md)
- [**Installing & building**](./docs/install.md)
- [**Open source fund**](./docs/open-source-fund.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).

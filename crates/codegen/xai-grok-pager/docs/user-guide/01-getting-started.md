# Getting Started

XAICode is the upstream terminal coding assistant with its hosted
account/control-plane surfaces removed. It keeps the original TUI, ACP/stdio,
headless mode, terminal and filesystem tools, sessions, MCP, plugins, hooks,
worktrees, memory, and permission controls.

## Install and configure

Build the workspace with a Rust toolchain and run the composition-root binary:

```sh
cargo build -p xaicode --bin xaicode
export CODING_AGENT_API_BASE_URL=http://127.0.0.1:8000/v1
export CODING_AGENT_API_KEY=your-key
```

The endpoint may be any OpenAI-compatible or self-hosted provider. The clean
runtime does not contact a vendor service, read account files, or perform an
automatic update. `OPENAI_API_KEY` remains a compatibility fallback.

Run the TUI:

```sh
xaicode
```

The original `xai-grok-pager` binary name remains available as a compatibility
alias. The user state directory is still selected by the upstream `GROK_HOME`
compatibility variable (default `~/.grok`) so existing local sessions can be
managed without a data migration.

## First session

There is no browser login screen. Configure a provider key before sending a
prompt. If a model has its own credentials, define it in `config.toml`:

```toml
[model.local]
model = "my-coder"
base_url = "https://inference.example/v1"
env_key = "MY_CODER_API_KEY"
```

Start a prompt and let the agent inspect files, run commands, and edit the
working tree. Local session history is stored under the state directory and can
be resumed with the normal session commands.

## TUI basics

- Type a prompt and press `Enter`.
- `Esc` cancels a running turn; `Ctrl+C` cancels or clears the prompt.
- `@path` attaches a file, line range, or directory.
- `Ctrl+O` toggles always-approve mode.
- `/new`, `/resume`, `/sessions`, `/model`, `/settings`, and `/doctor` remain
  local commands.
- `/usage` reports only the current session's locally recorded token/context
  usage. It does not query credits, subscriptions, or billing.

## Headless and ACP modes

Run one prompt without the full-screen UI:

```sh
xaicode -p "Run the test suite and summarize failures" --yolo
```

For editor integrations, use the local ACP server:

```sh
xaicode agent stdio
```

The ACP server exposes the generic API-key method and local tools. Hosted
relay, workspace, account, media, voice, feedback, trace-upload, telemetry,
and update methods are not part of this build.

## Project instructions

Place an `AGENTS.md` file in the repository (and optionally a user-level file
under the state directory) to describe coding conventions, test commands, and
review rules. The upstream-compatible `CLAUDE.md` and `CURSOR.md` discovery
paths remain available when enabled in local configuration.

## Next steps

Read [Custom models](11-custom-models.md) for OpenAI-compatible endpoints,
[Project rules](12-project-rules.md) for instruction files, [Plugins](09-plugins.md)
for local plugin sources, and [Headless mode](14-headless-mode.md) for scripts.
The repository-level [`CLEAN_BUILD.md`](../../../../../CLEAN_BUILD.md) lists the
exact source files changed from the upstream baseline.

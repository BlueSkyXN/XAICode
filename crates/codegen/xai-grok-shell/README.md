# XAICode shell

This crate is the agent/runtime layer used by the local XAICode build. It keeps
the upstream ACP, session, terminal, filesystem, MCP, plugin, hook, worktree,
memory, task, and subagent composition while keeping hosted control-plane
paths unreachable at the composition root.

## Quick start

The product command is `xaicode`. `xai-grok-pager` is retained only as a
compatibility command for clients that still invoke the upstream pager binary.

```sh
export CODING_AGENT_API_BASE_URL=http://127.0.0.1:8000/v1
export CODING_AGENT_API_KEY=your-key

xaicode
xaicode -p "Explain this codebase"
```

The supported runtime authentication mode is a provider API key. `OPENAI_API_KEY`
is accepted as a compatibility fallback. There is no browser login, account
session, device-code flow, or cached account credential path.

## Provider configuration

Inference is sent only to the endpoint selected by the user. A model can carry
its own endpoint, key, backend, authentication scheme, ordinary headers, query
parameters, environment-backed headers, and context-window metadata:

```toml
[model.local]
model = "my-coder"
base_url = "https://inference.example/v1"
env_key = ["MY_CODER_API_KEY", "OPENAI_API_KEY"]
api_backend = "responses"
auth_scheme = "bearer"
context_window = 128000

[model.local.extra_headers]
X-Client-Mode = "local"
```

Credentials remain in the selected provider path and are never written to an
account file or sent to a different endpoint. Vendor-hosted xAI/Grok domains
are rejected before request I/O; custom OpenAI-compatible endpoints are not
restricted to a first-party host.

## Headless mode

Headless mode accepts a prompt, runs configured local tools, and writes the
result to stdout:

```sh
xaicode -p "Review this change" --output-format json
xaicode -p "Continue where we left off" --resume <session-id>
xaicode -p "Stream the response" --output-format streaming-json
xaicode -p "Stream Messages-compatible events" \
  --output-format streaming-messages-json \
  --include-partial-messages
```

The supported output formats are `plain`, `json`, `streaming-json`, and
`streaming-messages-json`. `--include-partial-messages` affects only the last
format. `--tools`, `--disallowed-tools`, `--max-turns`, `--rules`, `--allow`,
`--deny`, `--yolo`, `--cwd`, `--resume`, `--continue`, and `--fork-session`
control local execution and session behavior.

## ACP and local sessions

Start the local ACP server with:

```sh
xaicode agent stdio
```

The compatibility command is:

```sh
xai-grok-pager agent stdio
```

ACP clients can use `session/new`, `session/load`, `session/resume`,
`session/list`, and `session/close`. Session updates are stored as local JSONL
under the compatibility state directory:

```text
~/.grok/sessions/<encoded-cwd>/<session-id>/
  summary.json
  updates.jsonl
  chat_history.jsonl
  plan.json
  rewind_points.jsonl
  signals.json
```

Set `GROK_HOME` to use another local state directory. Existing local sessions
remain readable; no remote session registry or cloud restore is consulted.
Use `xaicode sessions list`, `xaicode sessions search <term>`, and
`xaicode worktree gc --dry-run` for local inspection and cleanup.

## Local tools and integrations

The runtime retains terminal, filesystem, edit, git, LSP, search, worktree,
task, memory, plugin, hook, skill, and subagent tools. MCP transports and
third-party OAuth are available only for explicitly configured MCP servers;
MCP credentials are not reused as inference-provider credentials.

Local ACP, headless, TUI, session persistence, and worktree operations do not
initiate hosted relay, share, cloud workspace, managed catalog, remote search,
or telemetry requests. Legacy wire names and compatibility paths may remain in
protocol data, but their hosted implementations fail closed before credential
resolution or network I/O.

## Diagnostics and maintenance

```sh
xaicode doctor
xaicode inspect --json
RUST_LOG=debug xaicode -p "diagnostic prompt" 2> xaicode.log
```

The clean runtime does not perform installer downloads, automatic updates,
external telemetry, feedback uploads, or hosted usage queries. To change the
binary, use the repository's normal local build or release workflow.

Build and repository-level run instructions are kept in the root
[`README.md`](../../../README.md). The file-level cleanup map is in
[`CLEAN_BUILD.md`](../../../CLEAN_BUILD.md).

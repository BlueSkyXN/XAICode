# XAICode user guide

This guide belongs to the local-only derivative of the upstream Rust TUI.
The supported entry point is `xaicode`; only the historical `xai-grok-pager`
binary remains as a compatibility alias. No other product binary aliases are
declared.

## Start locally

```sh
export CODING_AGENT_API_BASE_URL=http://127.0.0.1:8000/v1
export CODING_AGENT_API_KEY=your-key
xaicode
```

The endpoint can be any OpenAI-compatible/self-hosted provider. A model entry
may override `base_url`, `api_key`, and `env_key` in `config.toml`.

## Supported surfaces

The original TUI, ACP/stdio, headless prompts, sessions, local persistence,
terminal and filesystem tools, MCP, plugins, hooks, worktrees, memory,
subagents, sandboxing, and plan/permission modes remain available.

## Deliberately removed

Browser/OIDC login and logout, cached account sessions, usage/credits/billing,
hosted relay/workspace/leader services, managed settings/model catalogs,
announcements/upsell links, voice/STT, media generation, feedback/trace upload,
Sentry/Mixpanel/OTLP/unified-log forwarding, and automatic updates are not
part of this build. Old hosted documentation files in this directory are
retained only as upstream source history; they are not valid instructions for
the clean runtime.

See the repository [`README.md`](../../../../../README.md) and
[`CLEAN_BUILD.md`](../../../../../CLEAN_BUILD.md) for the build and file-level
change map.

# Xaicode

This tree is a deliberately small, local-first derivative of the upstream
Rust terminal coding agent. It keeps the original TUI, ACP/stdio transport,
sessions, worktrees, MCP, plugins, hooks, and workspace tools, while removing
the hosted account and product-control paths.

The source baseline is upstream Git commit `500129c714ad1b10e6095481f4a8387a2ec52649`.
The composition-root binary is `xaicode`; `coding-agent` and the historical
`xai-grok-pager` binary names remain as compatibility aliases in the Cargo
package so downstream build scripts do not need to change at once.

## Local provider setup

The agent speaks to an OpenAI-compatible endpoint. Configure a provider in
`~/.grok/config.toml` or use environment variables. The `.grok` directory and
`GROK_HOME` variable are retained only as a storage-compatibility boundary so
existing sessions, worktrees, MCP, plugin, hook, and memory data are not lost;
new network/provider settings use the generic names below:

```sh
export CODING_AGENT_API_BASE_URL=http://127.0.0.1:8000/v1
export CODING_AGENT_API_KEY=your-key
# OPENAI_API_KEY is accepted as a compatibility fallback.
```

Provider-specific model entries may set `base_url`, `api_key`, or `env_key`.
No browser login, cached account session, hosted usage/billing query, hosted relay,
managed configuration fetch, or background update check is required.

## Build

Requirements are the same Rust and `protoc` prerequisites documented by the
upstream workspace:

```sh
cargo check -p xaicode
cargo build -p xaicode --bin xaicode
```

Run the TUI with:

```sh
cargo run -p xaicode --bin xaicode
```

For ACP clients and scripts, use `xaicode agent stdio` or
`xaicode agent headless`. Both are local stdio transports.

## What the clean build removes

- Grok/xAI browser login, logout, OAuth/OIDC, cached `auth.json` sessions, and
  account-specific ACP auth methods.
- Hosted usage, credits, billing, auto-top-up, subscription/paywall checks, sharing,
  hosted cloud/workspace/leader relay, and product announcements.
- Sentry, Mixpanel, unified-log forwarding, OTLP exporters, trace uploads,
  model-catalog prefetch, managed policy, and auto-update startup tasks.
- Voice/STT capture and the standalone voice-capture subprocess.

The detailed file-level change map is in [`CLEAN_BUILD.md`](CLEAN_BUILD.md).

## License

First-party code remains under the Apache License 2.0. Third-party notices are
preserved in [`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES) and the crate-local
notice files.

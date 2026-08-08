# XAICode

This tree is a deliberately small, local-first derivative of the upstream
Rust terminal coding agent. It keeps the original TUI, ACP/stdio transport,
sessions, worktrees, MCP, plugins, hooks, and workspace tools, while removing
the hosted account and product-control paths.

The source baseline is upstream Git commit `afbc0fb710320c7add294c2106d447ecc3e3af2e`
(public crate `1.0.0`, monorepo `SOURCE_REV`
`3e620a76a5f374ce644dc7c87f7e990c68348218`). XAICode versions this clean composition
independently as `0.2.0`.
The machine-readable provenance and binary policy are in [`UPSTREAM.toml`](UPSTREAM.toml).
The composition-root binary is `xaicode`; the historical `xai-grok-pager` binary remains as
a compatibility alias for downstream build and test tooling.

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

## Upstream maintenance

Validate the recorded baseline, binary matrix, updater removal, and production composition
markers without building the Rust workspace. The maintenance CLI requires Python 3.11 or newer:

```sh
python3 scripts/xaicode_maintenance.py check-contract
```

Compare the recorded baseline, the committed XAICode tree, and the pinned migration target in a
local sibling `grok-build` checkout:

```sh
python3 scripts/xaicode_maintenance.py audit-upstream
```

The audit is read-only and uses committed Git trees. Pass `--target <commit>` to override the
recorded candidate, `--format json` for automation, or `--list-paths` to include every path
changed by both XAICode and upstream.

The current phased migration, validation, stop, and rollback plan is in
[`UPSTREAM_MIGRATION.md`](UPSTREAM_MIGRATION.md). Its machine-readable status and fixed target are
recorded in `UPSTREAM.toml`.

## License

First-party code remains under the Apache License 2.0. Third-party notices are
preserved in [`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES) and the crate-local
notice files.

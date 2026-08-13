# XAICode

This tree is a deliberately small, local-first derivative of the upstream
Rust terminal coding agent. It keeps the original TUI, ACP/stdio transport,
sessions, worktrees, MCP, plugins, hooks, and workspace tools, while removing
the hosted account and product-control paths.

The source baseline is upstream Git commit `8a14c91d88875a831a38b3a066b1683116bcb31c`
(public crate `1.0.0`, monorepo `SOURCE_REV`
`27b3c66635e2c0bf213429a36ab916f25d59df20`). XAICode versions this clean composition
independently as `0.2.0`.
The machine-readable provenance and binary policy are in [`UPSTREAM.toml`](UPSTREAM.toml).
The composition-root binary is `xaicode`; the historical `xai-grok-pager` binary remains as
a compatibility alias for downstream build and test tooling.

## Install

Pushing a `vX.Y.Z` tag that matches the package version publishes a GitHub Release with:

- `xaicode-aarch64-apple-darwin.tar.gz`
- `xaicode-x86_64-unknown-linux-gnu.tar.gz`

The current release is [`v0.2.0`](https://github.com/BlueSkyXN/XAICode/releases/tag/v0.2.0).
Each archive contains only the `xaicode` binary.

```sh
# Apple Silicon example after downloading the macOS archive
tar -xzf xaicode-aarch64-apple-darwin.tar.gz
install -m 0755 xaicode "$HOME/.local/bin/xaicode"
xaicode --version
```

Linux GNU x86_64 uses `xaicode-x86_64-unknown-linux-gnu.tar.gz`. There is no npm package,
Homebrew formula, or in-app auto-update.

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

## Hosted production paths removed or disabled

- Grok/xAI browser login, logout, OAuth/OIDC, cached `auth.json` sessions, and
  account-specific ACP auth methods.
- Hosted usage, credits, billing, auto-top-up, subscription/paywall checks, sharing,
  hosted cloud/workspace/leader relay, and product announcements.
- Sentry, Mixpanel, unified-log forwarding, hosted/internal OTLP exporters,
  trace uploads, model-catalog prefetch, managed policy, and auto-update
  startup tasks. Generic customer OTLP remains only as an inert compatibility
  and test surface pending a separate product decision.
- Voice/STT capture and the standalone voice-capture subprocess.

Some legacy wire/configuration types and guarded helper modules remain for upstream and data
compatibility. Their hosted entry points are not registered by the product composition or return
before credentials and network I/O; the detailed map distinguishes those shells from physically
deleted code.

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

The completed `1.0.0` migration, validation, stop, and rollback record is in
[`UPSTREAM_MIGRATION.md`](UPSTREAM_MIGRATION.md). Its machine-readable status and integrated
target are recorded in `UPSTREAM.toml`.

The durable protected-intake process is in
[`docs/lts/upstream-maintenance.md`](docs/lts/upstream-maintenance.md). Weekly and manual
observations run read-only in GitHub Actions through `upstream-observation.yml`; they upload a
three-tree report but never modify product source or open a PR. The current fixed observation is
[`docs/lts/2026-08-23-upstream-observation.md`](docs/lts/2026-08-23-upstream-observation.md).
Rust compile, lint, tests, binary smoke, and packaging for an intake candidate are cloud-only on
the exact pushed SHA.

## License

First-party code remains under the Apache License 2.0. Third-party notices are
preserved in [`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES) and the crate-local
notice files.

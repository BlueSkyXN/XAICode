# XAICode Workspace Notes

This repo is the "xaicode" product source — a cleaned, local-first derivative of the upstream
Rust terminal coding agent. Do NOT re-introduce upstream's hosted paths (OAuth/OIDC login,
relay, leader, hosted workspace, credits/billing, telemetry firehose, auto-update).

## Build entrypoints

- Composition-root binary: `cargo build -p xaicode --bin xaicode --profile release-dist`
- Fast validation: `cargo check -p xaicode`
- Lint: `cargo clippy -p xaicode --all-targets -- -D warnings`
- Format: `cargo fmt --check --all`
- Tests: `cargo test -p xaicode --all-targets`

Do NOT run `cargo ... --workspace` unless you really need it — the workspace is large and slow.

## Rust toolchain

- Channel pinned in `rust-toolchain.toml` to `1.92.0`.
- Required components: `rustfmt`, `clippy`.
- Additional target in CI: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`.

## protoc

- `bin/protoc` is a DotSlash manifest. DotSlash fetches protoc v29.3 from GitHub Releases on first
  run and caches under `$HOME/.cache/dotslash`. No local install is needed, but CI runners must have
  network access to `github.com` (GitHub-hosted runners do by default).
- If protoc download fails offline, set `PROTOC` to a local protoc and `PROTOC_INCLUDE` to its
  `include/` directory.

## Brand

- Main binary name: `xaicode` (default-run).
- Compatibility alias: `xai-grok-pager` (kept for downstream build scripts).
- The `coding-agent` alias was intentionally dropped — that name is already used on npm/crates.io.
- Internal crate names keep the historical `xai-grok-*` prefix. Do NOT rename them unless you also
  update every importer across the 60+ crate workspace.
- User data dir remains `~/.grok` and `GROK_HOME` for storage compatibility (no network calls use it).

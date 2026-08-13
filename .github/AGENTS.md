# `.github` navigation card

This directory owns CI/release workflows. Read root `AGENTS.md` and this card before changing
commands, matrices, artifacts, tags or releases.

## Workflow contracts

- Composition-root release build: `cargo build -p xaicode --bin xaicode --profile release-dist`
- Fast validation: `cargo check -p xaicode`
- CI-equivalent lint: `cargo clippy -p xaicode --no-deps -- -D warnings`
- Format: `cargo fmt --check --all`
- Tests: `cargo test -p xaicode --all-targets`
- Maintenance contract: `python3 scripts/xaicode_maintenance.py check-contract`

Keep commands consistent with root. Add a full-workspace build only for a stated contract.

## Toolchain and protoc

- Channel pinned in `rust-toolchain.toml` to `1.94.0`.
- Required components: `rustfmt`, `clippy`.
- Additional target in CI: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`.
- `bin/protoc` is a DotSlash manifest. DotSlash fetches protoc v29.3 from GitHub Releases on first
  run and caches under `$HOME/.cache/dotslash`. No local install is needed, but CI runners must have
  network access to `github.com` (GitHub-hosted runners do by default).
- If protoc download fails offline, set `PROTOC` to a local protoc and `PROTOC_INCLUDE` to its
  `include/` directory.

## Release guardrails

- Release only `xaicode-*` artifacts built from the `xaicode` binary.
- After binary/startup changes, build and smoke `xai-grok-pager` too; release still contains
  `xaicode`.
- Do not add the upstream `grok` launcher, `@xai-official/*` packages, npm `postinstall`, or
  auto-update/install scripts.
- Do not create or move a version tag, change GitHub permissions/secrets, or add installers
  without explicit user authorization. Once an authorized `v*.*.*` tag is pushed,
  `release.yml` publishes the GitHub Release automatically.
- Validation stays `contents: read`; release write permission belongs only to release jobs.
- A release must align the XAICode Cargo package version, Git tag, `xaicode --version` output,
  provenance documentation, and artifact names.

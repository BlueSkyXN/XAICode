# `xai-grok-pager-bin` navigation card

This package is the XAICode composition root. Read this card before changing startup,
top-level CLI dispatch, product versioning, features, allocator setup, or binary declarations.
Key files are `Cargo.toml`, `src/main.rs`, and `build.rs`.

## Local invariants

- Package/default run and primary binary remain `xaicode`.
- The only current compatibility binary is `xai-grok-pager`; `coding-agent` and `grok` are not
  declared product binaries.
- Internal library/crate names may retain `xai-grok-*`; do not mass-rename importers here.
- Production startup uses local storage, no remote settings, generic API-key/provider setup,
  and local ACP/TUI/headless/server transports.
- Do not restore Grok/xAI login, cached account refresh, managed policy/model prefetch, hosted
  relay/workspace/leader, telemetry upload, feedback/upload, or auto-update startup tasks.
- Current leader entrypoints are clean-build-disabled. Enabling a demonstrably local-only IPC
  leader is a separate product change, not an incidental upstream merge resolution.
- `build.rs` controls user-facing version-with-commit text. Keep the XAICode public version
  distinct from the upstream compatibility/source version and verify the rendered output.

## Do not

- Do not add an upstream npm installer, updater dependency, relaunch path, or `grok` artifact.
- Do not remove the compatibility alias without checking PTY/test-support importers and release
  consumers.
- Do not make a hosted path merely hidden; reject it before credential loading or network I/O.

## Validation

Use the root remote-first CI/CD workflow. The commands below define the checks that GitHub
Actions must run; they are not permission to compile in this worktree.

- `cargo check -p xaicode`
- `cargo test -p xaicode --all-targets`
- `cargo build -p xaicode --bin xaicode`
- `cargo build -p xaicode --bin xai-grok-pager`
- Smoke `--version` and `--help` for both declared binaries when startup or naming changes.

# Upstream migration plan: grok-build 1.0.0

Status: **candidate** — the exact public source target and XAICode clean overlay are integrated on
the isolated candidate branch. Local validation is recorded separately; push, `main` integration,
tag, and release remain unauthorized.

This plan is the executable companion to [`UPSTREAM.toml`](UPSTREAM.toml),
[`CLEAN_BUILD.md`](CLEAN_BUILD.md), and the repository [`AGENTS.md`](AGENTS.md). The manifest now
records the candidate's integrated source baseline; this document preserves the historical base,
strategy, gates, and rollback boundary used to produce it.

## Fixed inputs

| Item | Value |
|---|---|
| XAICode HEAD at planning time | `96e3609111f61acdd0669d1a86fb02949f2fe454` (`v0.1.1`) |
| Integrated upstream base | `500129c714ad1b10e6095481f4a8387a2ec52649` / crate `0.2.114` |
| Integrated `SOURCE_REV` | `6372e41d828b8a6ee82c29e01a69e27ec895cca9` |
| Selected public target | `afbc0fb710320c7add294c2106d447ecc3e3af2e` / crate `1.0.0` |
| Target `SOURCE_REV` | `3e620a76a5f374ce644dc7c87f7e990c68348218` |
| Selected XAICode version | `0.2.0` |
| Implementation branch | `codex/upstream-1.0.0-clean` |

The public source target is an exact commit. The npm package also reports `1.0.0`, but its
`gitHead` does not equal the public source commit, so npm is distribution evidence rather than
source provenance. A newer upstream observation does not silently move this migration target.

## Current-state map

### Product entry points

- Cargo package/default run and primary binary: `xaicode`.
- Build/test compatibility binary: `xai-grok-pager`.
- `coding-agent` and `grok` are not declared product binaries.
- Composition root: `crates/codegen/xai-grok-pager-bin/src/main.rs`.
- Local ACP composition roots: `xai-grok-pager/src/acp/spawn.rs` and
  `xai-grok-shell/src/agent/app.rs`.

### Preserved behavior baseline

- Custom OpenAI-compatible providers retain URL, API/env key, auth scheme, ordinary headers,
  query parameters, environment-backed headers, model identity, and context window.
- TUI/dashboard, ACP stdio/headless, terminal/files/edit/git/LSP/search, worktrees, local
  sessions, persistence, memory, MCP, plugins, hooks, skills, tasks, and subagents remain.
- Third-party MCP OAuth remains available for explicitly configured MCP servers. It never falls
  back to an xAI issuer, account session, or `auth.json`.
- `~/.grok` and `GROK_HOME` remain local storage compatibility boundaries.

### Removed or unreachable behavior baseline

- Grok/xAI WebLogin, OIDC/device-code login, cached xAI sessions, account switching, and logout.
- Hosted credits, billing, subscription, auto-top-up, paywall, relay/share, cloud workspace,
  computer hub, remote session registry, and managed product controls.
- xAI Sentry, Mixpanel, firehose, trace/feedback upload, implicit OTLP, and remote unified logs.
- Vendor npm/postinstall, `grok` launcher, updater/download/relaunch, and official distribution.

### Existing evidence

- `CLEAN_BUILD.md` records the file-level clean overlay reapplied to the `1.0.0` source candidate.
- `UPSTREAM.toml` separates integrated provenance, latest observation, and product policy.
- `scripts/xaicode_maintenance.py check-contract` checks current composition and distribution
  markers without running Rust code.
- Current CI checks package `xaicode`; release builds and smokes both declared binaries while
  packaging only `xaicode`.

## Target state and non-goals

The target state is the public `grok-build` `1.0.0` implementation for generic/local behavior,
with XAICode's clean product policy re-applied at narrow composition, auth, endpoint, dispatch,
telemetry, and distribution seams.

Non-goals for this migration:

- No product-wide rewrite or mass rename of `xai-grok-*`, `x.ai/*`, `.grok`, or `GROK_HOME`.
- No xAI account login, hosted product control, vendor telemetry, updater, or official package.
- No live data migration, live provider mutation, deployment, publication, or credential change.
- No automatic enablement of local leader IPC, generic OTLP, first-party media/search/storage,
  or other currently disabled behavior; those remain separate product decisions.
- No claim that npm `1.0.0`, public source `1.0.0`, and XAICode `0.2.0` are one version line.

## Three-tree assessment

The read-only tree audit reports:

| Set | Changed paths |
|---|---:|
| XAICode overlay (`base -> XAICode`) | 297 |
| Upstream delta (`base -> target`) | 1,004 |
| Changed by both | 150 |
| XAICode-only | 147 |
| Upstream-only | 854 |

The overlap is concentrated in `xai-grok-shell` (59 paths) and `xai-grok-pager` (50 paths).
Consequently, whole-tree replacement, blind cherry-pick, or copying old clean files over the
new source are prohibited.

Path handling policy:

1. **Upstream-only:** import by default, then apply hosted/vendor deny scans and dependency review.
2. **Changed by both:** resolve semantically against current behavior; neither side wins by path.
3. **XAICode-only:** preserve unless the target makes it demonstrably obsolete; record any removal.
4. **Deleted clean surfaces:** keep deleted when they are updater/npm/vendor entry points; do not
   resurrect them merely because the target still contains them.

## Strategy

Use an isolated worktree and a real three-way ancestry base:

1. Record exact XAICode HEAD, dirty-state inventory, target commit, and rollback ref.
2. Add or reuse an `upstream` remote and fetch only the exact base and target objects.
3. Create `codex/upstream-1.0.0-clean` in a separate worktree from the recorded XAICode HEAD.
4. Add the old upstream base as an ancestry-only parent with Git's `ours` merge strategy; this
   must not change the XAICode file tree.
5. Merge the exact target without committing, allowing Git to use the real `0.2.114` base.
6. Resolve the 150 overlap paths by the phases below. Before completing the merge, prove there
   are no conflict markers and review every both-changed path.
7. Apply XAICode branding, product version, provenance, and distribution changes last.

The ancestry bridge is history plumbing, not evidence of an integrated target. Only the final
resolved tree plus validation can advance `UPSTREAM.toml`'s integrated baseline.

## Phased migration

### Phase 0: freeze and characterize

- Freeze the exact source and product refs; save the three-tree JSON/path inventory.
- Capture package/bin metadata and current production composition markers.
- Identify existing local-provider, ACP, persistence, and binary tests that remain valid.
- Gate: work occurs only in the isolated worktree; `main` and live `GROK_HOME` are unchanged.

Rollback: remove the isolated worktree/branch after preserving any requested patch evidence.

### Phase 1: workspace, toolchain, generated wire, and dependency closure

- Move Rust `1.92.0 -> 1.94.0` and reconcile workspace/lockfile using the target structure.
- Review the new `xai-grok-extra-ca` crate as generic TLS capability. It may be retained only for
  user-configured endpoints; the historical environment name can remain as compatibility.
- Import protobuf/ACP/tool capability changes with all importers together.
- Keep `xai-grok-update` and npm/postinstall outside the workspace and tracked tree.
- Gate: Cargo metadata resolves; updater/npm contract still fails closed; generated/wire changes
  have no unmatched importer.

Rollback: restore the phase-start workspace/toolchain/lockfile snapshot.

### Phase 2: provider, authentication, and network boundary

- Port generic retry/error/TLS/header/provider improvements.
- Preserve model-specific `base_url`, `api_key`, ordered `env_key`, `api_backend`, `auth_scheme`,
  `extra_headers`, `query_params`, and `env_http_headers`.
- Retain `AuthManager::new_local` production roots and API-key-only account-free startup.
- Reject upstream WebLogin, OIDC/device code, cached refresh, API-key login probes, account
  enrichment, and any xAI first-party fallback.
- Keep vendor endpoint/header filtering while allowing arbitrary user-selected non-vendor URLs.
- Gate: a loopback OpenAI-compatible provider receives the expected path, key, headers, query,
  streaming behavior, and no unrelated request; known vendor hosts fail before request I/O.

Rollback: restore the phase-start provider/auth files without changing saved credentials.

### Phase 3: ACP, headless, and session protocol

- Port `session/list`, `session/resume`, `session/close`, task lifecycle, version mismatch,
  startup hints, delivery-tool guidance, and headless streaming/reducer behavior.
- Preserve local stdio/headless behavior and existing output compatibility unless a new flag is
  explicitly selected; hosted leader/relay/account ACP methods remain rejected.
- Distinguish third-party MCP OAuth from account login throughout ACP initialization.
- Gate: local ACP method matrix passes; hosted/account prefixes fail before auth/network; stdio
  replay, cancellation, partial messages, and MCP connecting behavior are characterized.

Rollback: restore the phase-start ACP/headless/session wire closure as one unit.

### Phase 4: local persistence, search, restore, and worktrees

- Port search-index concurrency, bounded fork copying, local session lifecycle, restore fixes,
  git-head notifications, and worktree-registration preservation.
- Omit remote session synchronization, registry upload/download, cloud restore, and hosted share.
- Preserve old local database/session readability; do not open the live home during testing.
- Gate: a copied temporary `GROK_HOME` can list, resume, fork, close, reopen, and search sessions;
  shallow/large repo restore does not hang and remote sync performs no request.

Rollback: discard the temporary home and restore the phase-start persistence code; live data is
unchanged by construction.

### Phase 5: tools, permissions, process lifecycle, and subagents

- Port read-only tool capability, LSP diagnostics, process-tree reaping, bounded post-kill waits,
  background-task/subagent cancellation, admission limits, and typed security findings.
- Preserve local terminal/fs/git/worktree/MCP/plugin/hook/skill/task tools.
- Keep first-party web/media/voice/cloud-storage tools absent or fail closed unless they are a
  separately approved generic implementation.
- Gate: focused tool/process tests pass; cancel/kill cannot leak a registered child; readonly
  metadata and permission behavior agree across ACP and TUI.

Rollback: revert the phase slice; no live process registry or workspace is reused for tests.

### Phase 6: TUI, rendering, queue, dashboard, and local usage

- Port queue/cancel correctness, dashboard/session UX, theme/tmux/SSH, CJK copy, table reflow,
  permission-card, recap, error-banner, and rendering improvements.
- Adapt the new tabbed `/usage`/`/context`/`/session-info` modal to local token/context/session
  information only.
- Reject credits, subscription, auto-top-up, upsell, manage-account, feedback-upload, and hosted
  usage fetches even if their view/state types remain for compatibility.
- Gate: TUI snapshots/dispatch tests cover local views; opening local usage causes no billing or
  account request; dashboard/session actions preserve local state.

Rollback: restore the phase-start pager/render files; session data remains untouched.

### Phase 7: hosted, telemetry, workspace, and distribution closure

- Re-audit every production constructor and extension dispatcher after the preserved features
  compile; hiding a command is not sufficient.
- Keep hosted workspace/computer-hub/relay/leader/session-registry and remote managed controls
  unreachable. Port local git/workspace improvements only behind local composition roots.
- Keep external product telemetry and uploads disabled; local diagnostics may remain.
- Remove updater/npm/installer/official launcher from workspace, CLI, startup, and artifacts.
- Gate: production-like startup observes no vendor request and constructs no vendor sink; static
  URL/name scans are supporting evidence, not the sole proof.

Rollback: revert the closure slice and stop; do not release a candidate with an uncertain hosted
path.

### Phase 8: brand, version, provenance, and release candidate

- Keep package/primary binary `xaicode`; build/smoke `xai-grok-pager` as compatibility only.
- Keep `coding-agent` and `grok` undeclared and unshipped.
- Apply the selected XAICode version independently of upstream crate/npm versions.
- Update `SOURCE_REV`, Cargo versions/lockfile, `UPSTREAM.toml`, README, clean map, workflows, and
  this plan together only after all earlier gates pass.
- Gate: product package, tag plan, `--version`, artifact names, source commit, and `SOURCE_REV`
  agree; the release archive contains only the intended `xaicode` binary.

Rollback: retain the prior XAICode version and integrated baseline; no tag or artifact is created.

## Validation matrix

The authorized migration should run, in increasing scope:

1. `python3 scripts/xaicode_maintenance.py check-contract`
2. `cargo fmt --check --all`
3. Focused tests for each changed crate/behavior slice.
4. `cargo check -p xaicode`
5. `cargo clippy -p xaicode --no-deps -- -D warnings`
6. `cargo test -p xaicode --all-targets`
7. Build and smoke `xaicode` and `xai-grok-pager` with `--version` and `--help`.
8. Loopback custom-provider acceptance, including streaming and error mapping.
9. Production-like no-vendor-egress/account/hosted/telemetry checks.
10. Temporary-home persistence reopen/resume/search/fork acceptance.
11. Final three-tree review, `git diff --check`, conflict-marker scan, provenance readback, and
    release artifact inventory.

Local checks prove only the candidate checkout. GitHub CI, a published artifact, installation,
and live provider acceptance remain separate evidence stages.

## Candidate validation record: 2026-08-08

The following checks ran in the isolated candidate worktree with Homebrew `rustc`/`cargo`
`1.97.1`. The repository and GitHub workflows remain pinned to Rust `1.94.0`; exact-head
validation on that pinned toolchain requires GitHub CI after a separately authorized push.

| Gate | Command or evidence | Result |
|---|---|---|
| Provenance and clean contract | `python3 scripts/xaicode_maintenance.py check-contract --format json` | Passed, 123/123 checks |
| Formatting and patch sanity | `cargo fmt --check --all`; `git diff --check` | Passed |
| Composition compile | `cargo check -p xaicode` | Passed |
| CI-equivalent lint | `cargo clippy -p xaicode --no-deps -- -D warnings` | Passed; dependency warnings remain non-fatal and outside `--no-deps` enforcement |
| Composition tests | `cargo test -p xaicode --all-targets` | Passed, 35 tests for each declared binary target, 70 total |
| Generic provider config | `auth_scheme_is_preserved_in_model_override` | Passed; `[model.<id>].auth_scheme` reaches `ModelInfo` without a config warning |
| Custom-provider success boundary | `clean_custom_provider_is_the_only_egress_and_preserves_request_options` against the built `xaicode` binary | Passed; ordered env key, Bearer auth, ordinary/env-backed headers, Responses streaming, empty `configWarnings`, and zero proxy-trap connections |
| Custom-provider failure boundary | `clean_custom_provider_error_does_not_fall_back_or_egress` against the built `xaicode` binary | Passed; provider `401` remained the terminal error with no account/vendor fallback and zero proxy-trap connections |
| Provider query/header wire behavior | `cargo test -p xai-grok-sampler --test request_query_and_headers` | Passed; configured query parameters replace same-name base URL values and env-backed headers reach the request |
| Temporary persistence | Success-boundary E2E using a test-owned `GROK_HOME` | Passed; created a session, listed and FTS-searched it after process exit, then resumed the same session; live `GROK_HOME` was not opened |
| Binary build and smoke | `cargo build -p xaicode --bin xaicode --bin xai-grok-pager`; both binaries with `--version` and `--help` | Passed; both report XAICode `0.2.0`; primary usage is `xaicode`, and the compatibility help examples also use `xaicode` |
| Workflow syntax and boundary wiring | Ruby YAML parse of `ci.yml`/`release.yml`; maintenance workflow markers | Passed; CI builds the debug binary and release validates the `release-dist` binary with the same provider boundary tests |

GitHub Actions, `release-dist` artifacts, tags, Releases, installation, and a live external
provider were not run or changed by this local validation. Those remain separate delivery gates.

## Stop conditions

Stop and report exact evidence rather than weakening the product contract when:

- custom provider behavior would be lost or an xAI account credential could reach it;
- a hosted/account/billing/telemetry/updater path becomes production-reachable;
- ACP/protobuf/session schema changes lack a compatibility or rollback path;
- an old session database would be migrated in place or live `GROK_HOME` must be touched;
- a required dependency, binary alias, public interface, or outbound service needs a new user
  decision;
- the source target or provenance mapping changes during implementation.

## Rollback and delivery boundary

- The current `main` commit and `v0.1.1` tag are the rollback anchor.
- All migration changes stay on the isolated branch/worktree until explicit integration approval.
- No live data, credentials, remote branch, GitHub Release, package, or deployment is changed by
  the migration itself.
- A failed phase is reverted or the isolated worktree is discarded; it does not justify relaxing
  a clean boundary.
- Merge/push/tag/release remain separately authorized actions after candidate verification.

## Remaining delivery decisions

Source target `afbc0fb… / 1.0.0`, XAICode `0.2.0`, exact-object fetch, isolated worktree,
ancestry bridge, and Cargo build/test are authorized. The remaining separately authorized actions
are pushing the candidate branch, opening/merging a pull request, changing `main`, tagging,
creating a GitHub Release, or publishing/installing artifacts.

No `local/sdlc` ADS package exists in this repository. Durable architecture and dependency
constraints therefore remain in `AGENTS.md`, scoped navigation cards, `UPSTREAM.toml`,
`CLEAN_BUILD.md`, and this plan.

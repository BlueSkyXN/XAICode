# XAICode repository agent instructions

## Purpose

XAICode is a local-first derivative of `xai-org/grok-build`. It keeps the local agent
runtime and user-configured inference while excluding the Grok/xAI account and hosted
control plane. Compatibility names/types may remain, but production roots must keep hosted
paths unreachable.

## Codex startup behavior

- Codex normally starts from the repository root; this file is the startup-time router.
- A nested `AGENTS.md` is a navigation card for its subtree. Read it before changing files
  below that directory, even when it was not automatically loaded at startup.
- If several nested cards exist on the path to a target, read them from shallow to deep.
- Direct user instructions override this file. A more specific nested card overrides this
  file only for its own subtree.

## Current provenance and version layers

- Integrated source baseline: `grok-build@8a14c91d88875a831a38b3a066b1683116bcb31c`,
  upstream crate `1.0.0`, `SOURCE_REV` `27b3c66635e2c0bf213429a36ab916f25d59df20`.
- The XAICode composition package is `0.2.0`. The prior rollback tag is `v0.1.1`; the current
  latest Release is `v0.2.0`. Pushing a matching `v{product.version}` tag publishes a GitHub
  Release; creating or moving that tag still requires explicit authorization. Keep package, tag,
  `--version`, provenance, and artifact naming aligned when publishing.
- Keep upstream distribution, public source and XAICode product versions separate. The latest
  read-only public-source observation is commit
  `07b2f7144fd5c5c9d3dd1966937a87852d2dbdb8`, crate `1.0.8`, `SOURCE_REV`
  `956313d459bee15ae8f17bf73e0633605e18dddd`, 12 commits ahead of the integrated baseline.
  npm's stable `latest` tag points to `1.0.5` with a different `gitHead`; its `alpha` tag points
  to `1.0.8`, also without an exact public-commit mapping. Anchor syncs to an exact public commit,
  never an npm label alone.
- On completed migration, update `README.md`, `CLEAN_BUILD.md`, `SOURCE_REV`, Cargo versions
  and `UPSTREAM.toml` together. Retain both public upstream commit and monorepo `SOURCE_REV`.

## Directory map

| Path | Responsibility | Local AGENTS.md | Read when |
|---|---|---:|---|
| `.cargo/` | Local Cargo configuration and bundled tool lookup | No | Changing Cargo source, linker, or tool resolution |
| `.github/` | CI and release workflows | Yes | Any workflow, target matrix, artifact, release, or CI command change |
| `.workbuddy/` | Local-only agent state | No | Normally do not modify as product source |
| `bin/` | DotSlash tool manifests, notably `protoc` | No | Changing build-tool versions or download manifests |
| `crates/build/` | Rust build helpers and protobuf compilation | No | Changing code generation or `protoc` behavior |
| `crates/codegen/` | Main product crates and generated-source closure | No | Use the more specific cards listed below when applicable |
| `crates/codegen/xai-grok-pager-bin/` | XAICode composition root and installed binaries | Yes | Changing startup, CLI dispatch, product version, features, or binary names |
| `crates/codegen/xai-grok-shell/` | Agent runtime, providers, auth, ACP, sessions, relay and remote seams | Yes | Changing inference, auth, endpoints, runtime startup, ACP, sessions, leader, relay, or hosted seams |
| `crates/codegen/xai-grok-mcp/` | User-configured MCP transports and third-party OAuth | Yes | Changing MCP authentication, credentials, server discovery, or browser consent |
| `crates/codegen/xai-grok-telemetry/` | Local diagnostics and disabled upstream telemetry implementations | Yes | Changing logs, metrics, tracing, exporters, Sentry, or telemetry startup/shutdown |
| `crates/common/` | Shared protocol/runtime leaf crates | No | Changing shared wire or runtime contracts; validate all importers affected |
| `docs/lts/` | LTS upstream observation, intake policy, decision records and rollback gates | No | Changing observation cadence, intake classification, protected seams, validation or rollback policy |
| `prod/mc/` | Shared proxy and session wire types retained for compatibility | No | Changing serialized types or compatibility contracts |
| `scripts/` | Standard-library maintenance and audit CLI | No | Changing provenance, contract checks, output schema, or exit behavior |
| `third_party/` | Vendored third-party source and license material | No | Treat as read-only unless an explicit dependency/vendor update requires it |
| `local/` | Ignored reference snapshots and audit material, not product source | No | Read for comparison only; never treat a newer snapshot as the integrated baseline |
| Root docs/config | Workspace manifest, lockfile, provenance, clean-build map, and migration plan | No | Any workspace-wide dependency, source baseline, or documented contract change |

## On-demand cat protocol

Before editing a path with a local card, read it directly, for example:

```sh
cat crates/codegen/xai-grok-shell/AGENTS.md
```

Do not infer behavior from names: MCP browser OAuth is not Grok/xAI account login.

## Commands

The workspace is large. Local work is remote-first: run read-only Git, static contracts and
formatting locally; let GitHub Actions run compilation, lint, tests, binary smoke and packaging
on the exact candidate SHA. The Cargo commands below document the CI/release contract and are
not authorization to run them locally. Do not default to `--workspace`.

| Command | Purpose | Scope | Sandbox and dependency notes |
|---|---|---|---|
| `cargo check -p xaicode` | Fast compile validation | Composition package | GitHub Actions by default; local execution requires explicit authorization and can create a large `target/` |
| `cargo clippy -p xaicode --no-deps -- -D warnings` | CI-equivalent lint | Composition package | GitHub Actions by default; local execution requires explicit authorization |
| `cargo fmt --check --all` | Formatting validation | Workspace Rust files | Local-safe; no external service and does not populate `target/` |
| `cargo test -p xaicode --all-targets` | Composition-root tests | Composition package | GitHub Actions by default; may compile a large dependency closure |
| `cargo build -p xaicode --bin xaicode` | Debug build of primary binary | Composition package | GitHub Actions by default; local execution requires explicit authorization |
| `cargo build -p xaicode --bin xai-grok-pager` | Compatibility-alias build | Composition package | GitHub Actions by default when binary aliases or startup change |
| `cargo build -p xaicode --profile release-dist --bin xaicode` | Release artifact build | Composition package | Release workflow only by default; expensive and may need network/tools |
| `git diff --check` | Whitespace and patch sanity | Current worktree | Read-only with respect to tracked content |
| `python3 scripts/xaicode_maintenance.py check-contract` | Provenance and clean-contract check | Tracked source/config | Standard library only; no network or build |
| `python3 scripts/xaicode_maintenance.py audit-upstream [--target <commit>]` | Three-tree overlap audit | Local XAICode/upstream Git metadata | Read-only; defaults to the pinned `UPSTREAM.toml` migration target and needs a local upstream checkout containing the refs |

`bin/protoc` is a DotSlash manifest for `protoc` v29.3. If unavailable offline, report the
limitation; use an already available compatible `PROTOC`/`PROTOC_INCLUDE`, not a substitute.

## Remote-first CI/CD workflow

- Without explicit user authorization, do not run local commands that compile Rust or populate
  build caches, including `cargo check`, `cargo clippy`, `cargo test`, `cargo build`, `cargo run`,
  `cargo bench`, `cargo install` or wrappers that invoke them. Reading Cargo metadata may still
  resolve dependencies; prefer direct manifest inspection unless metadata is necessary.
- Local validation defaults to `git diff --check`, `cargo fmt --check --all`,
  `python3 scripts/xaicode_maintenance.py check-contract` and other standard-library/static
  checks that do not create `target/` artifacts.
- Push a candidate branch or update its pull request, then use `.github/workflows/ci.yml` as the
  compile/test authority. A candidate is CI-validated only when the completed successful run's
  `headSha` exactly matches the current remote branch and PR head.
- On CI failure, inspect the failed GitHub Actions job and logs, make the smallest source fix,
  and rerun CI. Do not silently fall back to a local full build merely to shorten diagnosis.
- `.github/workflows/release.yml` owns release-profile builds, cross-platform binaries, artifact
  packaging, and published GitHub Release creation on `v*.*.*` tags. Pushing a version tag is the
  human publish gate; the workflow then publishes the Release automatically. Manual
  `workflow_dispatch` on a non-tag ref only builds artifacts. Deployment, installers, and
  credential changes still require separate authorization.
- If local compilation is explicitly authorized, state the command and expected disk/time cost
  first. Use a task-scoped temporary `CARGO_TARGET_DIR` outside the repository with
  `CARGO_INCREMENTAL=0`; remove only that task-created directory after verification. Never clean
  a pre-existing shared Cargo cache or another worktree's `target/` without explicit permission.

## Clean product contract

### Preserve the local and generic data plane

Unless the user accepts a behavior change, preserve:

- TUI/dashboard; ACP stdio/headless/local server; terminal/fs/edit/git/LSP/search/worktree/
  task tools; sessions/persistence/compaction/queue/rewind/memory/local search.
- MCP, plugins, hooks, skills, non-vendor marketplaces and subagents.
- Custom provider `base_url`/`api_base_url`, `api_key`, ordered `env_key`, `api_backend`,
  `auth_scheme`, ordinary `extra_headers`, `query_params`, `env_http_headers`, context window
  and model identity.
- Supported Chat Completions, Responses and Messages-compatible API shapes.

Never send an xAI account/session credential to a custom endpoint. Provider auth helpers are
generic candidates, but if currently disabled, enable them only as a characterized slice with
no first-party default or browser/account fallback.

### Keep the Grok/xAI hosted control plane unreachable

Production startup/local use must not initiate:

- Grok/xAI WebLogin, account OAuth/OIDC/device code, cached xAI sessions, `auth.json`
  adoption, account switching/logout/re-login.
- Hosted billing/credits/subscription/paywall/auto-top-up/upgrade (local context usage stays).
- Hosted relay/share/cloud sandbox/workspace/computer hub/session registry, managed modes,
  announcements, campaigns, policy/settings or first-party model-catalog fetches.
- xAI Sentry/Mixpanel/firehose/log forwarding, uploads, feedback or implicit telemetry.
- Upstream npm/postinstall/`grok` launcher/auto-update/download/relaunch paths.
- First-party voice/STT/media/search/GCS/S3 unless a later approved slice proves a generic,
  explicitly configured implementation.

Legacy types/wire names/tests may remain. Disabled paths must return before credential
resolution or network I/O; hiding UI alone is insufficient.

### Do not over-strip generic authentication and integrations

- Preserve third-party MCP OAuth for configured servers; never use xAI account issuers or
  credentials. API/env keys, ordinary headers and provider helpers are not account login.
- Local diagnostics are allowed; external xAI sinks are not. Generic OTLP and local-only
  leader IPC are separate decisions and must not return incidentally.
- Preserve local workspace code; distinguish it from hosted workspace/relay clients.

### Network boundary

- Inference goes only to the selected provider; MCP/plugins only to explicit non-vendor
  sources. Preserve ordinary headers but inject no `x-grok-*`/xAI session markers.
- xAI/Grok hosted URLs must resolve disabled or fail before request construction. Negative
  fixtures may contain them. Loopback-provider startup must contact nothing unrelated.

### Persistence boundary

- Keep `~/.grok`/`GROK_HOME`; do not move live data during source updates. Production local
  auth must not read/write xAI account `auth.json`.
- Preserve sessions, worktrees, MCP credentials, plugins, hooks, skills and memory. Test a
  copied temporary home, including session-search concurrency/schema; never live data.

## Brand, binaries and versions

- Package/primary binary: `xaicode`; compatibility binary: `xai-grok-pager`.
- `coding-agent` is undeclared; do not document/release it without an explicit decision.
- Do not ship an unqualified `grok` binary or `@xai-official/*` package from this repository.
- Keep internal `xai-grok-*` crate names and compatible wire identifiers. Mass-renaming the
  60+ crate closure creates upstream merge churn without improving runtime purity.
- Keep release artifact names under the `xaicode-*` brand.
- Keep XAICode, upstream source and npm versions separate. Before release, align package/tag
  and verify `xaicode --version`.

## Upstream synchronization procedure

This is an incremental migration, not a tree overwrite.

### Inputs

Exact target public commit/`SOURCE_REV`, distribution metadata, current clean baseline, clean
worktree, isolated implementation worktree, and no live deployment/data mutation.

### Ordered steps

1. Record public commit, source crate version, `SOURCE_REV`, npm version/time and mapping
   confidence separately.
2. Run the maintenance audit to inventory `base -> XAICode`, `base -> target`, and their
   intersection.
3. Import upstream-only changes; reapply clean behavior by semantic slice. Explicitly review
   both-changed files; never overwrite new code with an old clean file.
4. Stabilize workspace/toolchain/lockfile/wire/binaries, then generic provider behavior.
5. Re-establish account, hosted remote, billing UI, telemetry/upload, updater and media/search/
   storage boundaries independently; validate preserved local features after each slice.
6. Apply brand/version/provenance last, then focused tests, production-config smoke, diff and
   rollback review.

### Completion evidence

A source update is complete only when all of the following agree:

- Target commit/`SOURCE_REV` and clean-overlay review agree.
- Focused Cargo tests and both binary checks pass at the candidate commit.
- Loopback custom provider succeeds without xAI auth; hosted gates fail before network and
  startup observes no vendor request.
- Temporary persistence reopens; live data is untouched.
- Docs, clean map, versions/tag plan and artifact names agree.

### Stop and escalation conditions

Stop the migration and report evidence when:

- Release-to-source mapping is missing; schema/auth/wire/binary compatibility lacks rollback;
  generic provider behavior would be lost; or hosted code becomes reachable.
- The change needs internal renames, live-data moves, alias changes, dependencies or outbound
  services without a user decision.
- Validation needs unreviewed third-party install/execution beyond pinned Cargo/DotSlash;
  state the command and side effects first.

## Global engineering rules

- Make the smallest sufficient change within the current migration slice.
- Prefer upstream structure and APIs for preserved generic behavior; keep XAICode-specific
  policy concentrated at composition roots, dispatch gates and small adapters.
- Do not copy stale local reference snapshots into the product. `local/` is evidence only.
- Root `Cargo.toml` is generated upstream; reconcile target manifest plus minimal XAICode delta.
- Preserve wire compatibility unless accepted; review protobuf/ACP changes explicitly.
- Add concise comments only where the clean reachability boundary is otherwise non-obvious.
- Do not commit secrets, provider keys, private endpoints, `~/.grok` contents or local
  reference archives.
- Never revert unrelated work. Use an isolated worktree for large upstream updates.

## Do not

- Do not merge or copy the upstream tree over XAICode without a three-way base comparison.
- Do not reintroduce an xAI login path merely to make upstream tests compile.
- Do not disable all OAuth, auth, telemetry, leader, workspace or remote-named code by name;
  classify first-party hosted behavior separately from explicit generic/local behavior.
- Do not use `#[cfg(test)]` as the only proof that a production fail-closed guard works.
- Hidden UI, loopback constants or unit tests alone are not no-egress evidence.
- Do not run `cargo ... --workspace` by default; use it only when a workspace-wide contract
  actually changed and state the expected cost.
- Do not tag, publish, push, create a release, run an installer, deploy, or modify credentials
  without explicit authorization.
- Audit upstream read-only unless fetch/checkout/build/execution is explicitly authorized.

## Validation standards

Validation depth follows the changed boundary. Map these checks to the corresponding GitHub
Actions jobs by default; do not execute local compile/test commands unless explicitly authorized:

1. Rust: format plus narrow relevant test.
2. Composition/provider/auth: check, CI-equivalent clippy and package tests when available.
3. Binary/release: build/smoke both binaries; verify help, version and packaged aliases.
4. Clean boundary: production-like endpoint/auth/ACP gates, not only `cfg(test)` paths.
5. Provider: loopback server validates URL/API/credentials/headers/query/stream/error behavior.
6. Persistence: temporary `GROK_HOME` reopen/resume/search only.
7. Upstream: inventory, wire/generated/dependency review, `git diff --check`, skipped commands
   and risks.

Passing local checks proves only the local candidate. GitHub CI, published artifacts, runtime
installation and live custom-provider acceptance are separate evidence stages.

## Notes for future agents

- The completed `0.2.114 -> 1.0.0` source migration overlapped the clean patch heavily in
  shell/pager. Future syncs must repeat the three-tree semantic review rather than assume the
  now-integrated tree can be overwritten.
- The direct-child `afbc0fb -> 8a14c91` update removed the legacy managed-MCP config client.
  Keep third-party MCP OAuth, but retain the production fail-closed guards around the separate
  hosted gateway catalog/tool-call helpers.
- `UPSTREAM.toml` separates the integrated baseline from the latest observed candidate; update
  the latter only after a fresh read-only upstream check.
- `docs/lts/upstream-maintenance.md` is the stable LTS intake runbook; each observation must pin
  exact public source and distribution coordinates without automatically changing product source.
- `UPSTREAM_MIGRATION.md` is the current executable migration plan; keep its status, fixed refs,
  phases, gates, stop conditions, and open decisions truthful as work advances.
- Local unified logs are not hosted forwarding; MCP OAuth is not xAI account login.

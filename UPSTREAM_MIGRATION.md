# Upstream incremental migration: `afbc0fb` → `8a14c91`

Status: **complete** — the exact direct-child upstream delta and subsequent consumer-first source
cleanup were merged through PR #2. The integrated tree is `main@7dec356645be0b61c34d074c9fbaa4be246e5153`,
tagged and released as `v0.2.0`. Exact-head GitHub Actions evidence is recorded below;
installation, deployment, and live-provider acceptance remain separate delivery stages.

This document is the executable companion to [`UPSTREAM.toml`](UPSTREAM.toml),
[`CLEAN_BUILD.md`](CLEAN_BUILD.md), and [`AGENTS.md`](AGENTS.md).

## Fixed inputs

| Item | Value |
|---|---|
| XAICode baseline | `e2afb878cf56cf3ec8235a0fa58e76960454fe3a` on `main` |
| Previously integrated upstream | `afbc0fb710320c7add294c2106d447ecc3e3af2e` |
| Previous `SOURCE_REV` | `3e620a76a5f374ce644dc7c87f7e990c68348218` |
| Selected public target | `8a14c91d88875a831a38b3a066b1683116bcb31c` |
| Target `SOURCE_REV` | `27b3c66635e2c0bf213429a36ab916f25d59df20` |
| Public upstream crate | `1.0.0` |
| XAICode product version | `0.2.0` |
| Implementation branch | `codex/source-clean-recovered` |
| Recovery archive identifier | ZIP comment `a6d0e193f16ebe61f8250a20840c0d420dcc4b9b` (not a Git object) |
| Recovery archive SHA-256 | `0d64c19c31569d62319632573da1da36fd2874c3667bece199d2ae278b3685a8` |

`8a14c91` is the direct child of `afbc0fb`; no ancestry bridge or version-label inference is
needed. The npm package remains `@xai-official/grok@1.0.0`, published
`2026-08-07T01:15:46.097Z`, with `gitHead`
`3cd0d0cbcebeb5b94a2830326ceb466d4341a5c4`. It is distribution evidence only and does not
map exactly to the public source commit.

### Later upstream observation

A read-only check on `2026-08-23T07:14:19Z` found public upstream `main` at
`19d42e35c07a9c9244f03f6df0c4c353f970d4f9`, 11 commits ahead of this integrated target. That
commit records `SOURCE_REV` `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa` and crate `1.0.6`.
The npm stable `latest` tag is `1.0.5` and npm `alpha` is `1.0.8`; their `gitHead` values do not
map exactly to the observed public commit.

Those 11 commits are not part of the integrated XAICode source. They require a new incremental
migration with a fresh three-tree review: 1,055 upstream paths changed, 301 overlap the XAICode
clean overlay, and upstream `1.0.6` contains a breaking subagent contract change. The fixed
observation and initial feature classification are in
[`docs/lts/2026-08-23-upstream-observation.md`](docs/lts/2026-08-23-upstream-observation.md).

## Product boundary

### Preserved

- TUI/dashboard, ACP stdio/headless/local server, terminal/files/edit/git/LSP/search,
  worktrees, local sessions, replay, persistence, compaction, memory, plugins, hooks, skills,
  tasks, subagents, and user-configured MCP servers.
- Custom provider `base_url`/`api_base_url`, ordered `env_key`, API key, `api_backend`,
  `auth_scheme`, ordinary and environment-backed headers, query parameters, context window,
  and model identity.
- Chat Completions, Responses, and Messages-compatible request shapes.
- Standards-based OAuth for explicitly configured third-party MCP servers.
- `~/.grok`/`GROK_HOME` as the existing local persistence boundary.

### Physically absent

- Hosted billing/share extensions, managed MCP gateway consumers and support crate, several
  hosted remote/workspace clients, workspace server/probe binaries, and product skills client.
- Vendor telemetry, Sentry, Mixpanel, internal OTLP, trace/feedback upload, remote-log upload,
  GCS/S3/storage clients, and voice/STT runtime.
- The updater crate, active updater startup integration, npm launcher/meta-packages,
  postinstall logic, and installer scripts.

### Retained only for compatibility and unreachable in production

- Grok/xAI auth flow/config types, login/logout command and wire identifiers, and account
  metadata carriers. The product composition uses local API-key auth and starts no browser,
  OIDC, device-code, cached-session, account-switching, or billing flow.
- Leader protocol/relaunch helpers, remote/session-registry shells, legacy auto-update helper,
  and hosted settings/model-catalog carriers. Composition roots disable them before auth or
  network work; local screen-mode relaunch is unrelated to product update installation.
- The local-only feedback manager and generic customer OTLP compatibility/test surface. Feedback
  is persisted only in local session data; OTLP production activation remains inert pending a
  separate product decision and has no vendor defaults.
- Legacy configuration and ACP carriers needed for parsing and wire compatibility. Local/plugin
  MCP methods and standards-based third-party OAuth remain active.

### Product identity

- Package and primary binary: `xaicode`.
- Compatibility binary: `xai-grok-pager`.
- Forbidden product binaries remain `coding-agent` and `grok`.
- XAICode stays `0.2.0`; upstream source and npm versions remain separate evidence.

## Three-tree assessment

| Set | Paths |
|---|---:|
| Upstream delta (`afbc0fb -> 8a14c91`) | 157 |
| XAICode clean overlay (`afbc0fb -> e2afb878`) | 335 |
| Upstream-only | 123 |
| Changed by both | 34 |
| XAICode-only | 301 |

The upstream delta contains 12 added, 2 deleted, and 143 modified files. Git's real
three-way merge produced nine textual conflicts:

- `xai-grok-pager/docs/user-guide/07-mcp-servers.md`
- `xai-grok-pager/src/acp/spawn.rs`
- `xai-grok-pager/src/app/event_loop.rs`
- `xai-grok-pager/src/views/mcps_modal.rs`
- `xai-grok-shell-session-support/src/managed_mcp.rs`
- `xai-grok-shell/src/agent/mvp_agent/acp_agent.rs`
- `xai-grok-shell/src/agent/mvp_agent/agent_ops.rs`
- `xai-grok-shell/src/mcp_doctor.rs`
- `xai-grok-shell/src/session/acp_session_impl/mcp.rs`

Every conflict was resolved semantically. No old XAICode file was copied wholesale over a
newer upstream implementation.

## Imported runtime slices

| Slice | Integrated behavior | XAICode-specific decision |
|---|---|---|
| Memory trace | signal-safe bounded wait and topology tests | local only |
| Worktrees | preserve standalone worktree identity across cwd/load/resume/fork | retained |
| `.envrc` | bounded evaluation, process-group termination, output limits | retained |
| Session replay | faster raw JSONL replay, bounded completion window, relocation handling | retained |
| Goal/queue | Send Now steering/interjection and authoritative TUI restyling | retained |
| Session deletion | drain active/pending subagents before deleting session state | retained |
| Headless HITL | answer known ExtMethods and return explicit errors for unknown methods | retained |
| Startup/exit | structural non-blocking startup, load barrier, bounded flush/join/watchdog | no product updater task |
| Models | bounded first-catalog wait and local `x.ai/models/list` | remote catalog gate remains `false` |
| MCP | remove legacy `grok_com_*` managed-config client and reactive reauth | third-party OAuth kept; hosted gateway consumers physically removed |
| Tools | skill-path suggestions, read/ask/task/OOM-child improvements | local tools retained |
| Telemetry | startup ownership and slow-phase timing | local diagnostics retained; generic customer OTLP production activation remains inert; vendor sinks removed |

The exported OOM delta only resets a child process's inherited `oom_score_adj` when its parent
was protected elsewhere, and it is opt-in through `GROK_TOOLS_RESET_CHILD_OOM`; XAICode does
not set that variable itself. XAICode therefore does not claim that this commit independently
protects the tools server or provides complete OOM-kill attribution.

## Conflict decisions

### Startup and updater

`acp/spawn.rs` keeps `AuthManager::new_local` and does not start account refresh, managed
policy fetch, online model prefetch, or background model refresh. `event_loop.rs` imports the
new startup barrier and exit watchdog while continuing to omit the background updater
receiver. The updater crate/npm tree remains outside the clean workspace.

### Models and providers

The upstream model generation fence, bounded wait, and `x.ai/models/list` handler are kept.
`resolve_remote_fetch_enabled()` remains an unconditional production `false`, so local/custom
catalog use does not activate the xAI models or settings endpoints. The provider schema and
sampling backends were unchanged by this upstream commit.

### MCP

The obsolete hosted `/mcp/configs` client, `grok_com_*` prefix classification, injected xAI
headers, proactive refresh, and reactive managed reauth are removed with upstream. Explicitly
configured servers, including a server whose user-chosen name starts with `grok_com_`, now
follow the ordinary local/third-party MCP path.

The old hosted gateway compatibility module and its support crate are physically removed;
compatibility wire/config carriers remain parseable without a hosted consumer. Agent startup
and the MCP extension expose only local/plugin definitions. This does not disable third-party
MCP OAuth.

### Telemetry and hosted ACP

The new startup timing state feeds the retained local diagnostic path. The vendor telemetry,
Sentry, Mixpanel, internal OTLP, and upload implementations are physically absent. Generic
customer OTLP parsing, redaction, and exporter tests remain for compatibility, but production
resolution and initialization both return before exporter construction pending a separate
product decision. It has no vendor endpoint, account identity, or remote-policy gate. The extension dispatcher
allows local ACP methods such as models/session/MCP/tools while account, billing, hosted
workspace, relay, media, upload, trace, and product-telemetry methods have no production
dispatcher or network consumer.

## Consumer-first source cleanup

After the direct-child migration, the candidate was reduced from runtime guards to physical
removal wherever local or generic behavior did not depend on the hosted implementation. This
removed hosted account/billing/share extensions, managed MCP gateway consumers, remote session
and workspace clients, vendor telemetry and upload pipelines, voice/STT runtime, the updater
crate, and installer paths. Compatibility carriers and guarded helper shells remain only where
the local crate graph still needs them.

The cleanup deliberately does not rename `config.toml`, `GROK_HOME`, existing fields, serde
representations, defaults, environment bindings, merge precedence, or raw roundtrips. Generic
provider authentication, user-configured headers/query parameters, all three inference API
shapes, and standards-based third-party MCP OAuth remain part of the preserved data plane.

## Validation matrix

Local work is limited to read-only/static checks that do not compile Rust or populate `target/`:

```sh
python3 scripts/xaicode_maintenance.py check-contract
python3 scripts/xaicode_maintenance.py audit-upstream --target <exact-public-commit>
git diff --check
cargo fmt --check --all
```

Run compilation, lint, tests, binary smoke, provider-boundary tests, packaging, and release
artifacts only in GitHub Actions on the exact candidate SHA:

```sh
cargo check -p xaicode
cargo clippy -p xaicode --no-deps -- -D warnings
cargo test -p xaicode --all-targets
cargo build -p xaicode --bin xaicode --bin xai-grok-pager
```

Then smoke both binaries; run the loopback custom-provider/provider-header tests; verify
production managed-gateway and other vendor traps receive zero connections; and reopen/resume/
search a session under a test-owned temporary `GROK_HOME`.

## Baseline completion record: 2026-08-13

| Gate | Result |
|---|---|
| Fixed provenance | rollback `main@e2afb878`; integrated upstream `8a14c91`; target `SOURCE_REV` confirmed |
| Previous exact-head CI | Run `31291320896` succeeded for maintenance, Linux/macOS check and tests, and provider boundary |
| Previous exact-head release build | Run `31263126749` succeeded for Linux/macOS build, dual-binary smoke, and provider boundary |
| Three-tree audit | 157 upstream paths, 34 overlap paths, real direct-child merge |
| Conflict-marker and patch scan | Passed; no exact merge markers and `git diff --check` is clean |
| Current source-clean overlay audit | 547 paths relative to rollback `e2afb878`: 533 tracked-file changes plus 14 recovered Rust additions |
| Clean contract | Passed, 750 checks, including hosted-surface absence, production-inert OTLP guards, auth-file no-access markers, and preserved provider/MCP/config invariants |
| Static workspace graph | 76 members, 82 local path dependencies and 1,215 lock packages; no missing or dangling local references |
| Rust format and patch sanity | `cargo fmt --check --all` and `git diff --check` passed with Homebrew Rust 1.97.1 |
| Rust check and lint | `cargo check -p xai-grok-shell`; `cargo check -p xaicode`; `cargo clippy -p xaicode --no-deps -- -D warnings` passed; dependency warnings remain non-fatal and outside the CI-equivalent `--no-deps` lint scope |
| Composition and auth tests | `cargo test -p xaicode --all-targets` passed 35 tests for each declared binary target (70 total); `cargo test -p xai-grok-shell auth::recovery::tests --lib` passed 18 tests after preserving transient recovery semantics; shell `cfg(test)` compilation succeeds |
| Pager session-load barrier | `cargo test -p xai-grok-pager session_load_barrier::tests` passed 13 focused tests after removing the stale hosted-announcements test module declaration |
| Recovered module compilation | `cargo test -p xai-grok-pager --lib --no-run` and `cargo test -p xai-computer-hub-sdk --lib --no-run` passed, covering the recovered pager and local computer-hub module roots |
| Dual-binary build and smoke | `xaicode` and `xai-grok-pager` built and passed `--version`/`--help`; both identify XAICode `0.2.0`; no `grok` or `coding-agent` product binary is declared or built |
| Custom-provider boundary | Two production-binary loopback E2Es passed: ordered env-key Bearer auth, Responses streaming, ordinary/env-backed headers, local persistence, provider `401` terminal behavior, and zero auxiliary/vendor trap connections |
| Provider query/header wire behavior | `cargo test -p xai-grok-sampler --test request_query_and_headers` passed; configured query parameters and env-backed headers reached the request |
| Temporary persistence | Test-owned `GROK_HOME` created, listed, FTS-searched, and resumed the same session after process exit; the live home was not opened |
| Production auth/OTLP negative smoke | With a sentinel `auth.json` and OTLP switches/exporters pointed at a loopback collector, production `xaicode inspect --json` made zero collector connections; auth bytes, mode, and mtime were unchanged and no `auth.json.lock` was created |
| Hosted-gateway proof | Production consumers and support crate are absent; the static contract pins their absence and preserved third-party MCP paths |
| GitHub Actions | PR head `67dbf6d2eeba8439c38138a6508871554833c07d` passed CI runs `31660999818` and `31660996964`; merged `main@7dec356645be0b61c34d074c9fbaa4be246e5153` passed run `31662999949`; tag `v0.2.0` passed Release run `31664646242` |

The historical local checks used Homebrew Rust/Cargo 1.97.1; the authoritative compile/test and
release evidence above used the repository-pinned Rust 1.94.0 in GitHub Actions. Those runs prove
only the integrated `v0.2.0` baseline. They do not validate the later observed upstream commit,
installation, deployment, or live-provider acceptance.

## Stop conditions

Stop rather than weakening the product boundary if:

- custom provider behavior is lost or an xAI account credential can reach it;
- a hosted account, billing, relay, workspace, telemetry, updater, or managed gateway path
  becomes production-reachable;
- ACP/session schema changes lack a rollback path;
- validation would require live credentials or live `GROK_HOME`;
- binary aliases, version, dependencies, or outbound services need a new product decision;
- the exact public target moves during implementation.

## Rollback and delivery boundary

- Rollback anchor: `main@e2afb878cf56cf3ec8235a0fa58e76960454fe3a`.
- Integrated anchor: `main@7dec356645be0b61c34d074c9fbaa4be246e5153`, tag and Release `v0.2.0`.
- The later `19d42e3` observation changes provenance records only; it does not import upstream
  source, alter live data or credentials, create a product tag, install, or deploy anything.
- Every future intake uses a new isolated branch/worktree and the LTS runbook. Merge, tag,
  Release, installation, and deployment remain separately authorized and independently verified.

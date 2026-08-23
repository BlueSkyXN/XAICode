# Clean-build change map

This document records the consumer-first changes made on top of the original
Rust checkout. The source baseline is Git commit
`8a14c91d88875a831a38b3a066b1683116bcb31c` (public crate `1.0.0`, monorepo
`SOURCE_REV` `27b3c66635e2c0bf213429a36ab916f25d59df20`).
The machine-readable baseline and binary policy are in [`UPSTREAM.toml`](UPSTREAM.toml).

The direct-child update from `afbc0fb710320c7add294c2106d447ecc3e3af2e`
was merged with Git's real ancestry base. It imports the upstream session replay,
non-blocking startup, bounded shutdown, goal/interjection, worktree, `.envrc`,
subagent-drain, skill-discovery, and local tool fixes. The merge also accepts
upstream's removal of the legacy `grok_com_*` managed-MCP config client while
retaining third-party MCP OAuth. The hosted MCP gateway catalog, tool-call
helpers, product skills client, and session-support crate are absent from this
source tree; the remaining ACP MCP list/call methods are local/plugin paths.

## Composition roots and CLI

- `crates/codegen/xai-grok-pager-bin/src/main.rs`
  - Removes startup auth refresh, remote prefetch, managed-policy validation,
    Sentry, hosted firehose initialization, and the active product updater/check/
    download/relaunch integration.
    The headless/CLI composition retains generic customer OTLP lifecycle calls,
    but production config resolution and initialization both return before
    exporter construction pending a separate product decision.
  - Makes `agent`, `headless`, and the default TUI use local stdio/ACP only.
  - Rejects leader/relay execution, removes login/logout/setup/share/trace/update
    command dispatch, and removes the voice capture subprocess entry point.
  - Uses `xaicode` in version/startup text and a generic HTTP client type.
- `crates/codegen/xai-grok-pager/src/app/cli.rs`
  - Removes login/logout/setup/share/trace/update/workspace command variants and
    the re-auth/vendor URL flags.
  - Adds `--api-base-url`/`CODING_AGENT_API_BASE_URL`; legacy relay fields remain
    unparsed only to preserve Rust struct compatibility.
  - `crates/codegen/xai-grok-pager/src/app/dispatch/router.rs` keeps stale
    auth actions as wire-compatible no-ops; they cannot start OAuth, device
    code, logout, or hosted account switching.
  - Hides leader/login/telemetry/update controls from the public CLI.
- `crates/codegen/xai-grok-pager-bin/Cargo.toml`
  - Adds the `xaicode` binary name and keeps `xai-grok-pager` as a compatibility alias;
    removes the updater dependency.
- `crates/codegen/xai-grok-pager/Cargo.toml`
  - Removes the updater dependency.
- `Cargo.toml`
  - Removes the standalone upstream auto-update crate from the workspace so
    the clean build cannot compile or publish its installer/update commands.
- `crates/codegen/xai-grok-pager/npm/`
  - Removes the upstream `@xai-official/grok` meta-package, platform binary
    packages, postinstall trampoline, and npm registry/version-install logic.
    Distribution is source/binary based; installing the clean agent cannot
    silently create a vendor-specific `~/.grok` launcher or updater path.

## Agent bootstrap and authentication

- `crates/codegen/xai-grok-pager/src/acp/spawn.rs`
  - Uses `AuthManager::new_local`; removes refresh listeners, managed-policy
    sync, OTLP gate setup, and online model catalog refresh.
- `crates/codegen/xai-grok-pager/src/acp/mod.rs`
  - Keeps remote settings empty and uses account-free auth for compatibility
    bridge code; startup authentication can only use a generic API key.
- `crates/codegen/xai-grok-shell/src/auth/manager.rs`
  - Adds `AuthManager::new_local`, which never reads `GROK_AUTH` or `auth.json`.
- `crates/codegen/xai-grok-shell/src/auth/flow.rs`
  - Makes every interactive, device-code, refresh, logout, and external auth
    entry point fail closed in production; the old implementation is retained
    only for unit-test compatibility.
- `crates/codegen/xai-grok-shell/src/auth/config.rs`
  - Removes the built-in hosted OIDC/OAuth defaults and ignores provider
    environment overrides; legacy issuer/scope symbols point to a non-routable
    local sentinel and no accounts-app CORS origin is accepted.
- `crates/codegen/xai-grok-shell/src/agent/auth_method.rs`
  - Exposes only `api_key`, reading `CODING_AGENT_API_KEY` or `OPENAI_API_KEY`.
  - Removes interactive/cached-token fallback from the active builder.
- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs`
  - Rejects old interactive auth requests and account/billing/cloud/share/
    telemetry ACP extensions before their legacy handlers run.
  - Stops managed MCP catalogs, hosted announcements, heap-profile monitors,
    and account session initialization in the active bootstrap path.
- `crates/codegen/xai-grok-shell/src/agent/config.rs`
  - Defaults endpoints to a local OpenAI-compatible base URL and generic env
    names; managed-config resolution is a disabled local sentinel.
  - Forces account OAuth, hosted telemetry, trace upload, feedback, recap, and
    voice resolution off in the clean runtime; the separate generic customer
    OTLP table remains compatible and defaults off.
- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs`
  - Makes remote settings fetch, feedback, trace/diagnostic upload, telemetry
    client construction, image generation, and video generation fail closed
    even when a stale hosted caller bypasses the normal feature gates.
- `crates/codegen/xai-grok-shell/src/agent/init.rs`
  - Makes bootstrap local-only: no remote-settings prefetch, managed policy,
    campaign sync, or auth-refresh model watcher; storage is forced local.
- `crates/codegen/xai-grok-shell/src/agent/app.rs`
  - Active stdio/headless paths use the local ACP runner; the legacy relay
    implementation is left unreachable for source compatibility.
- `crates/codegen/xai-grok-env/src/lib.rs` and
  `crates/codegen/xai-grok-shell/src/tools/config.rs`
  - Replace production inference/assets/relay/gateway and in-process search
    defaults with local loopback sentinels; vendor URLs cannot be inherited
    from the environment/config merge.
- `crates/codegen/xai-grok-shell/src/relay/sync.rs`,
  `remote/{chat_models_client,conversations_client,workspaces_client,agent}.rs`, and
  `agent/chat_modes.rs`
  - The hosted relay synchronization, model/chat/workspace clients, sandbox agent,
    and hosted chat-mode implementations are physically absent.
- `crates/codegen/xai-grok-shell/src/relay/mod.rs`, `remote/client.rs`,
  `leader/server.rs`, and `agent/session_registry_client.rs`
  - Compatibility shells remain compiled, but the product composition never starts
    leader/relay mode and retained remote/session-registry entry points fail before
    auth resolution or network I/O.
- `crates/codegen/xai-grok-shell-base/src/util/changelog.rs` and
  `crates/codegen/xai-grok-workspace/src/handle.rs`
  - Changelog reads are cache-only; the hosted workspace connection entry point
    is absent (local tools use the in-process handle instead).
- `crates/codegen/xai-grok-workspace/src/bin/workspace_server.rs` and its probe
  - The hosted workspace-server/probe binaries are physically absent. Their
    configuration and RPC carriers remain parseable for compatibility without
    a hosted process or socket entry point.

## TUI/runtime privacy and product surfaces

- `crates/codegen/xai-grok-pager/src/app/mod.rs` and `app/event_loop.rs`
  - Remove startup remote prefetch/auth refresh/update receiver and force leader
  mode, login UI, voice, privacy/upsell controls, and remote settings off.
- `crates/codegen/xai-grok-pager/src/views/{welcome/mod.rs,mcps_modal.rs,
  privacy_banner.rs}`, `app/dispatch/status.rs`, and `slash/commands/docs.rs`
  - Remove hosted upgrade/credits/connectors/legal/docs links and turn the stale
    actions into local no-ops or empty link values.
- `crates/codegen/xai-grok-pager/src/slash/registry.rs` and
  `crates/codegen/xai-grok-pager-minimal/src/{auth,welcome}.rs`
  - The hosted share command implementation is absent. The registry hard-hides
    retained compatibility commands for account, voice, media, recap, hosted-product,
    and login/logout even when stale metadata asks to reveal them. The
    local dashboard/session switcher remains feature-flagged and available;
    `/usage` remains available but is local session token/context usage only;
    consumer credits and billing are not shown. The minimal renderer shows
    generic provider setup instead of a browser login.
- `crates/codegen/xai-grok-pager/src/app/app_view.rs` and
  `app/acp_handler/settings.rs`
  - The billing dispatcher and subscription watcher modules are absent. The retained
    view/settings compatibility layer clears hosted credit/paywall metadata and keeps
    local session usage when old wire settings arrive.
- `crates/codegen/xai-grok-pager/src/tracing.rs`, `app/signal_handler.rs`,
  `src/unified_log.rs`, and the deleted telemetry paths
  `crates/codegen/xai-grok-telemetry/src/{client.rs,http.rs,sentry.rs,otel_layer/}`
  - Remove the hosted telemetry client, Sentry, internal OTLP layer, and
    account-bearing HTTP shim. Local formatter/error cleanup, local logs,
    W3C trace context, and stable session correlation remain. The retained
    `external/` module is a generic customer OTLP compatibility/test surface:
    its stable `GROK_EXTERNAL_OTEL` switch (with `XAICODE_EXTERNAL_OTEL`
    accepted as an alias), parser, redaction, and exporter tests remain, but
    production config resolution and initialization are both inert. No hosted
    policy or account identity is attached.
- `crates/codegen/xai-grok-models/default_models.json` and
  `xai-grok-models/src/lib.rs`
  - Replace hosted model defaults/catalog copy with a generic `local-model`
    placeholder and local config/provider wording.
- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs`
  - The extension gate rejects hosted account, credit/usage, relay, cloud,
    media, feedback, upload, trace, announcement, and telemetry operations
    before dispatch. Local session usage, terminal, filesystem, git, MCP,
    plugin, hook, skill, worktree, task, and memory extensions keep the
    original ACP wire names for compatibility and remain in-process.
- `crates/codegen/xai-grok-voice/src/config.rs`
  - Retains only the voice configuration carrier for dependency and config
    compatibility; microphone, audio, bearer, and STT runtime code is absent.
- `crates/codegen/xai-grok-tools/src/implementations/{grok_build,web_search}`
  - Media clients are disabled at construction; hosted web-search endpoints
    are rejected; the generic web-fetch user agent and default allowlist no
    longer contain vendor domains.

- `crates/codegen/xai-grok-sampler/src/client.rs`
  - Rejects direct construction with a vendor-hosted inference URL and strips
    old `x-grok-*`/XAI marker headers in production, while preserving ordinary
    user-supplied provider headers and all three generic API shapes.

- `crates/codegen/xai-file-utils/src/{trace_context.rs,upload_config.rs,workspace_classifier.rs}`
  - Retains local classifier/W3C trace-context helpers and the inert upload
    configuration carrier. GCS/S3/storage clients, upload queues,
    multipart/presign paths, and remote existence checks are physically absent.

- `crates/codegen/xai-grok-pager/src/memory_trace.rs`
  - Retains local memory-trace capture/export. The old trace-command module, direct
    upload helpers, and upload-method resolution are physically absent.

- `crates/codegen/xai-grok-plugin-marketplace/src/lib.rs`,
  `xai-grok-shell/src/extensions/marketplace.rs`, and `xai-grok-shell/src/plugin.rs`
  - Official hosted marketplace auto-registration, upgrade CTAs, and vendor
    source URLs are disabled. Local marketplace files and explicitly configured
    non-vendor sources still work.

- `crates/codegen/xai-grok-shell/src/session/feedback_manager.rs`,
  `xai-grok-pager/src/app/dispatch/notes.rs`, and the voice crate
  - Hosted trace/feedback upload clients, hosted voice probing, and provider voice
    authentication are physically absent. The feedback manager writes only to local
    session persistence; local notes, memory-trace export, and media rendering remain.

- `crates/codegen/xai-grok-pager/src/app/dispatch/tests/mod.rs`
  - Hosted upload, billing/paywall, media, and subscription assertions are
    removed with their production surfaces; local `/usage` behavior and token
    counts remain covered by local status/session tests.

- `crates/codegen/xai-grok-pager/docs/user-guide/{01-getting-started,02-authentication,04-slash-commands,05-configuration,11-custom-models,14-headless-mode,17-sessions,24-monitoring-usage}.md`
  - Rewrites the supported setup, API-key, local usage, model, headless, and
    diagnostics guidance so it no longer instructs users to log in, query
    hosted billing, upload traces, or enable telemetry/update services.

- `crates/codegen/xai-grok-pager/scripts/`
  - Removes the upstream shell/PowerShell installers. The clean package has no
    vendor download URL, installer bootstrap, or self-update entry point.

## Explicitly preserved

The cleanup does not rewrite the agent core. Terminal/file tools, generic
OpenAI-compatible inference, sessions and local persistence, the local
dashboard/session picker, MCP, plugins, hooks, worktrees, memory, ACP/stdio,
and the TUI remain in the original crate layout. Legacy vendor modules remain
compiled only where the upstream crate graph requires their types; the
composition roots, tool constructors, and extension dispatcher keep them
unreachable or fail closed.

Names such as `xai-grok-*`, `x.ai/*`, and the historical `GROK_*` symbols may
still appear in compatibility types, ACP wire identifiers, migration parsers,
deny-list tests, or disabled legacy modules. They are not active provider
endpoints, login methods, telemetry sinks, or hosted product actions in the
clean composition roots.

## Hosted MCP absence and local preservation contract

The source-clean MCP boundary is intentionally consumer-first:

- `xai-grok-shell-session-support` and the remote product skills client are
  physically absent; no gateway catalog/cache/client or hosted tool dispatch is
  present in the shell, pager, or tools production paths.
- `session/mcp_sources.rs` is the neutral local/plugin/client merge seam. Its
  source precedence, disabled-server handling, folder-trust gate, and plugin
  OAuth collection remain active. A configured `grok_com_*` name is treated as
  an ordinary local name by the MCP catalog and pager.
- `xai-grok-mcp` still owns stdio, HTTP, and SSE transports plus standards-based
  OAuth for explicitly configured third-party servers. Local sessions,
  persistence, plugins, skills, and MCP tool registration remain available.
- The `[managed_mcps]` config carriers and their serde/default/env/precedence
  behavior remain parseable for compatibility. They are inert without a hosted
  consumer; this cleanup does not rename `config.toml`, fields, or environment
  variables.

The read-only maintenance contract checks these absence and preservation
markers with `python3 scripts/xaicode_maintenance.py check-contract`; use
`git diff --check` and `cargo fmt --check --all` for source-level validation.

## Computer-hub local boundary

The workspace/computer-hub cleanup is consumer-first as well:

- The workspace client, MCP adapter, hosted workspace server/proxy supervisor,
  socket/pool/demux/handshake runtime, and SDK donation modules are physically
  absent. `WorkspaceOps` has only its local filesystem/git/worktree/session
  implementation; hosted dispatch is not registered.
- `HubConfig`, workspace RPC DTOs, transport discriminants, auth-provider
  carriers, and other compatibility shapes remain parseable. No
  `config.toml` path, field name, serde/default behavior, environment name,
  merge precedence, or raw roundtrip is changed by this boundary.
- The SDK retains `LocalRegistry`, `ToolHandle`/`ErasedTool`, local session and
  default-extension handling, `ToolHarness`, and `ToolServerHandler`/protocol.
  `xai-grok-mcp` stdio, HTTP, SSE, and third-party OAuth remain available, as
  do local workspace FS, git, worktree, permission, persistence, and MCP
  registration.

The cleanup deliberately preserves the existing `config.toml` schema and
carrier fields, including `[voice]`, `[ui].voice_*`, image/video feature
settings, and upload/trace compatibility values. Their serde/default/env/merge
and raw-roundtrip behavior remains intact, but no production client, queue,
audio pipeline, or remote exporter is constructed from those values.

## Verification boundary

The image used to create the initial clean source did not contain `cargo`, `rustc`, or
`rustfmt`, so the recovered import initially had only static evidence. The assembled candidate
was subsequently validated locally with Homebrew Rust/Cargo 1.97.1: the maintenance contract,
formatting and patch checks, focused package compilation and lint, composition tests, both
binary builds and CLI smoke tests, loopback custom-provider success/error paths, query/header
transport, temporary-home session create/list/search/resume, production `auth.json` no-access,
and production-inert OTLP behavior all passed.

The authoritative repository toolchain remains Rust 1.94.0. PR head
`67dbf6d2eeba8439c38138a6508871554833c07d` and merged
`main@7dec356645be0b61c34d074c9fbaa4be246e5153` passed the GitHub Actions CI workflow on that
toolchain; tag `v0.2.0` passed the cloud Release workflow. Those results prove only the integrated
baseline. Later upstream observations, installation, live-provider acceptance, and deployment
remain separate evidence stages. Future Rust compilation and tests for LTS intake run only in
GitHub Actions on the exact candidate SHA.

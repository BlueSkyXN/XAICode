# Clean-build change map

This document records the minimal-invasion changes made on top of the original
Rust checkout. The source baseline is Git commit
`500129c714ad1b10e6095481f4a8387a2ec52649`.

## Composition roots and CLI

- `crates/codegen/xai-grok-pager-bin/src/main.rs`
  - Removes startup auth refresh, remote prefetch, managed-policy validation,
    Sentry, OTLP/firehose initialization, and all update-check/relaunch code.
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
  - Adds the `xaicode` binary name and keeps `coding-agent` plus
    `xai-grok-pager` as compatibility aliases; removes the updater dependency.
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
  - Forces account OAuth, telemetry, trace upload, feedback, recap, and voice
    resolution off in the clean runtime.
- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs`
  - Makes remote settings fetch, feedback, trace/diagnostic upload, telemetry
    client construction, image generation, and video generation fail closed
    even when a stale caller bypasses the normal feature gates.
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
- `crates/codegen/xai-grok-shell/src/relay/sync.rs`, `remote/{client,
  chat_models_client,conversations_client,workspaces_client}.rs`, and
  `leader/server.rs`
  - Hosted share/remote/workspace/leader clients are fail-closed (no relay task,
    empty share URL, non-routable backend constants, and no hosted computer hub).
- `crates/codegen/xai-grok-shell/src/agent/session_registry_client.rs`,
  `remote/agent.rs`, and `agent/chat_modes.rs`
  - Session-registry, sandbox-management, and hosted chat-mode requests return
    a local-build-disabled error before auth resolution or network I/O.
- `crates/codegen/xai-grok-shell-base/src/util/changelog.rs` and
  `crates/codegen/xai-grok-workspace/src/handle.rs`
  - Changelog reads are cache-only; the hosted workspace connection entry point
    is fail-closed (local tools use the in-process handle instead).
- `crates/codegen/xai-grok-workspace/src/bin/workspace_server.rs`
  - Keeps the upstream binary/argument surface for source compatibility, but
    changes its identity and default hub to loopback, reports no hosted
    capabilities, and exits before daemonization, credential loading, remote
    hub connection, or telemetry initialization.

## TUI/runtime privacy and product surfaces

- `crates/codegen/xai-grok-pager/src/app/mod.rs` and `app/event_loop.rs`
  - Remove startup remote prefetch/auth refresh/update receiver and force leader
  mode, login UI, voice, privacy/upsell controls, and remote settings off.
- `crates/codegen/xai-grok-pager/src/views/{welcome/mod.rs,mcps_modal.rs,
  privacy_banner.rs}`, `app/dispatch/status.rs`, and `slash/commands/docs.rs`
  - Remove hosted upgrade/credits/connectors/legal/docs links and turn the stale
    actions into local no-ops or empty link values.
- `crates/codegen/xai-grok-pager/src/slash/registry.rs`,
  `slash/commands/{share}.rs`, and `crates/codegen/xai-grok-pager-minimal/src/{auth,welcome}.rs`
  - Hard-hide account, sharing, voice, media, recap, hosted-product, and
    login/logout commands even when stale metadata asks to reveal them. The
    local dashboard/session switcher remains feature-flagged and available;
    `/usage` remains available but is local session token/context usage only;
    consumer credits and billing are not shown. The minimal renderer shows
    generic provider setup instead of a browser login.
- `crates/codegen/xai-grok-pager/src/app/app_view.rs`,
  `app/acp_handler/settings.rs`, `app/dispatch/billing.rs`, and
  `app/subscription.rs`
  - Hide hosted credits/billing, ignore stale billing responses, retain local
    session usage, and disable the subscription watcher even when old
    metadata/settings arrive.
- `crates/codegen/xai-grok-pager/src/tracing.rs`, `app/signal_handler.rs`,
  `src/unified_log.rs`, and `crates/codegen/xai-grok-telemetry/src/{client.rs,
  external/mod.rs,sentry.rs,session_ctx.rs,unified_log.rs,id.rs}`
  - Leave local formatter/error cleanup in place but make product telemetry,
    Sentry, OTLP, Mixpanel, ACP unified-log forwarding, stable machine-ID
    caching, and shutdown flushing no-ops.
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
- `crates/codegen/xai-grok-voice/src/{config.rs,pipeline.rs,stt/streaming.rs}`
  - Retains the upstream types for dependency compatibility but makes voice
    pipeline and STT connection entry points return a local-build-disabled
    error before microphone/network work.
- `crates/codegen/xai-grok-tools/src/implementations/{grok_build,web_search}`
  - Media clients are disabled at construction; hosted web-search endpoints
    are rejected; the generic web-fetch user agent and default allowlist no
    longer contain vendor domains.

- `crates/codegen/xai-grok-sampler/src/client.rs`
  - Rejects direct construction with a vendor-hosted inference URL and strips
    old `x-grok-*`/XAI marker headers in production, while preserving ordinary
    user-supplied provider headers and all three generic API shapes.

- `crates/codegen/xai-file-utils/src/storage_client.rs`, `gcs.rs`, and `s3.rs`
  - Keep the upstream storage types and local mock seams, but every public
    remote upload, download, presign, existence-check, and multipart entry
    point fails before building a production network request.

- `crates/codegen/xai-grok-pager/src/trace_cmd.rs`
  - Converts the trace command to local export; direct upload helpers and
    upload-method resolution are production fail-closed stubs.

- `crates/codegen/xai-grok-plugin-marketplace/src/lib.rs`,
  `xai-grok-shell/src/extensions/marketplace.rs`, and `xai-grok-shell/src/plugin.rs`
  - Official hosted marketplace auto-registration, upgrade CTAs, and vendor
    source URLs are disabled. Local marketplace files and explicitly configured
    non-vendor sources still work.

- `crates/codegen/xai-grok-pager/src/trace_cmd.rs`,
  `xai-grok-pager/src/app/dispatch/notes.rs`, and the voice crate
  - Trace upload, feedback submission, hosted voice probing, and provider
    voice authentication are local-build stubs; trace export remains local.

- `crates/codegen/xai-grok-pager/src/app/dispatch/tests/mod.rs`
  - Leaves the upstream hosted billing/paywall tests in the checkout for
    provenance, but excludes that removed-surface suite from compilation;
    local `/usage` behavior is covered by the status-dispatch tests.

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

## Verification limitation

The execution image does not contain `cargo`, `rustc`, or `rustfmt`, so the
package includes static checks (`git diff --check`, source/JSON inspection) but
not a compiled binary. Build commands above should be run in a Rust toolchain
environment before distribution.

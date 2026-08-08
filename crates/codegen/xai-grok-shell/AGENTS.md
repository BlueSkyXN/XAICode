# `xai-grok-shell` navigation card

Runtime, providers, auth, ACP, persistence and hosted seams. Read before changing
`src/agent/`, `src/auth/`, `src/remote/`, `src/relay/`,
`src/leader/`, sessions, endpoints, extension dispatch or startup.

## Local invariants

- Preserve custom URL, API/env key, backend/auth scheme, ordinary/query/env-backed headers,
  context window and model.
- Never send a cached/session/xAI credential to a custom provider endpoint.
- Account auth stays local/API-key based. WebLogin, OIDC/device code, cached-token refresh,
  `auth.json` adoption and account switching remain unreachable.
- Provider auth helpers are generic. Change them separately and prove no first-party default,
  browser fallback or session-token leakage.
- Hosted share/relay/sandbox/workspace/registry, managed config/model, billing, feedback,
  upload and product extensions fail before auth/network.
- Preserve local sessions, ACP, tools, worktrees, MCP/plugins/hooks/skills, tasks/subagents,
  memory, usage and wire names.
- Keep local workspace code; reject hosted workspace/computer-hub clients.
- `~/.grok`/`GROK_HOME` remains a persistence boundary. Tests use a temporary home, never
  live data.

## Required review for upstream merges

- Review composition-root call sites, not only constants/UI.
- Separate generic providers from account auth when resolving config/auth/sampler conflicts.
- Review ACP gates as a table: hosted/account denied, local kept.
- Treat search schema/concurrency, protobuf and version mismatch as contracts.

## Do not

- Do not copy an old clean file wholesale over a newer upstream runtime implementation.
- Do not use `cfg(test)` legacy behavior as proof of the production fail-closed path.
- Do not disable external-auth/leader/remote/workspace/telemetry code by name; classify
  local/generic behavior first.

## Validation

Use root commands plus narrow tests. Provider/auth needs loopback and production-like
no-vendor-egress checks; persistence needs temporary `GROK_HOME` reopen/resume/search.

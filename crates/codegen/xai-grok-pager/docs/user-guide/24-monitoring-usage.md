# Monitoring and Usage

XAICode records local session state needed for the TUI, ACP, context
compaction, and `/usage` display. It does not send product analytics, Sentry,
Mixpanel, OTLP, trace uploads, feedback, heap profiles, or unified-log events
to a hosted service.

## Local diagnostics

- `xaicode doctor` checks terminal, filesystem, sandbox, and MCP setup.
- `xaicode inspect --json` reports resolved local configuration and model
  entries.
- `RUST_LOG=debug` enables local diagnostic logging to stderr.
- `/usage` shows the in-process token/context ledger for the current session;
  it is not account credit, subscription, or billing usage.
- `/trace` (where exposed by a compatibility client) exports a local file only.

## Legacy settings

The upstream configuration schema still accepts legacy `[telemetry]`,
`[features] telemetry`, `trace_upload`, feedback, and external-OTEL keys so
old config files continue to parse. Production resolution forces these values
off and clears their endpoints, credentials, intervals, and headers. Setting a
legacy flag cannot re-enable network telemetry.

## Application-level observability

If an organization needs monitoring, collect the process's local stdout/stderr,
exit codes, session files, or provider-side metrics from the explicitly
configured OpenAI-compatible endpoint. XAICode itself does not proxy those
records through a vendor control plane.

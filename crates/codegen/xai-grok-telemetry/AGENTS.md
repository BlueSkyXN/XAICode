# `xai-grok-telemetry` guardrail card

This crate retains upstream telemetry types while the clean product keeps external product
telemetry disabled. Read this card before changing telemetry construction, exporters, Sentry,
Mixpanel, unified logs, startup hooks, shutdown flushing or stable identifiers.

## Why this is high-risk

- A small constructor/startup change can silently restore outbound product telemetry.
- Local diagnostic logging and hosted unified-log forwarding share historical names.
- Unit tests may intentionally exercise legacy implementations that production must not reach.

## Required before changes

- Identify every production call site that constructs or starts the changed component.
- Classify the sink as local file diagnostics, explicit user-configured generic export, or
  xAI/Grok product telemetry.
- Prove disabled/default behavior before adding an opt-in path.

## Do not

- Do not initialize xAI Sentry, Mixpanel, firehose, OTLP, trace upload or unified-log forwarding.
- Do not materialize a vendor endpoint, API key, account identifier or stable machine ID in
  production-disabled mode.
- Do not remove local formatter/error diagnostics just because a module is named telemetry.
- Do not enable a generic user-configured OTLP exporter incidentally during an upstream sync;
  that is a separate product decision requiring explicit opt-in and no vendor default.

## Validation

Use the root remote-first CI/CD workflow. Run focused crate tests, root `xaicode` checks and the
production-like startup/shutdown smoke in GitHub Actions; local compilation requires explicit
authorization. The smoke must observe no external telemetry connection attempts; local
unified-log output may remain.

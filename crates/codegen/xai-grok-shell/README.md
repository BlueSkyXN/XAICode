# Xaicode shell

This crate is the agent/runtime layer used by the local Xaicode build.
It keeps the upstream ACP, session, terminal, filesystem, MCP, plugin, hook,
worktree, and memory composition, while the hosted account/control-plane
paths are disabled at the composition root.

The supported runtime authentication mode is a provider API key:

```sh
export CODING_AGENT_API_BASE_URL=http://127.0.0.1:8000/v1
export CODING_AGENT_API_KEY=your-key
# OPENAI_API_KEY is accepted as a compatibility fallback.
```

The shell never performs browser login, reads cached account credentials,
queries usage or billing, starts a hosted relay, downloads a managed model
catalog, sends telemetry, or checks for updates. Model entries may instead
provide their own OpenAI-compatible `base_url`, `api_key`, and `env_key`.

Build and run instructions are kept in the repository-level
[`README.md`](../../../README.md). The file-level cleanup map is in
[`CLEAN_BUILD.md`](../../../CLEAN_BUILD.md).
